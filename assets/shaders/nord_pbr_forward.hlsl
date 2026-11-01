cbuffer CameraData : register(b0)
{
    column_major float4x4 model;
    column_major float4x4 viewProjection;
    column_major float4x4 normalModel;
    float4 cameraPosition;
    column_major float4x4 lightViewProjection;
};

cbuffer MaterialData : register(b1)
{
    float4 baseColorFactor;
    float4 materialFactors;
    // x: normal scale, y: exposure, z: shadow texel size, w reserved.
    float4 materialOptions;
};

Texture2D<float4> baseColorTexture : register(t0);
Texture2D<float4> metallicRoughnessTexture : register(t1);
Texture2D<float4> normalTexture : register(t2);
Texture2D<float> directionalShadowTexture : register(t3);
SamplerState materialSampler : register(s0);

static const float PI = 3.14159265358979323846;

struct VertexInput
{
    float3 position : POSITION;
    float3 normal : NORMAL;
    float4 tangent : TANGENT;
    float2 texCoord : TEXCOORD0;
    float3 instanceTranslation : INSTANCE_TRANSLATION;
    float3 instanceTint : INSTANCE_TINT;
};

struct VertexOutput
{
    float4 position : SV_Position;
    float3 worldPosition : TEXCOORD0;
    float3 worldNormal : TEXCOORD1;
    float4 worldTangent : TEXCOORD2;
    float2 texCoord : TEXCOORD3;
    float3 instanceTint : TEXCOORD4;
    float4 lightPosition : TEXCOORD5;
};

VertexOutput VSMain(VertexInput input)
{
    VertexOutput output;
    float4 worldPosition = mul(model, float4(input.position, 1.0));
    worldPosition.xyz += input.instanceTranslation;
    const float3 worldNormal = normalize(mul((float3x3)normalModel, input.normal));
    float3 worldTangent = mul((float3x3)model, input.tangent.xyz);
    worldTangent = normalize(worldTangent - worldNormal * dot(worldNormal, worldTangent));

    output.position = mul(viewProjection, worldPosition);
    output.worldPosition = worldPosition.xyz;
    output.worldNormal = worldNormal;
    output.worldTangent = float4(worldTangent, input.tangent.w);
    output.texCoord = input.texCoord;
    output.instanceTint = input.instanceTint;
    output.lightPosition = mul(lightViewProjection, worldPosition);
    return output;
}

float3 FresnelSchlick(float cosine, float3 f0)
{
    const float fresnel = pow(saturate(1.0 - cosine), 5.0);
    return f0 + (1.0 - f0) * fresnel;
}

float DistributionGgx(float normalHalf, float alpha)
{
    const float alphaSquared = alpha * alpha;
    const float denominator = normalHalf * normalHalf * (alphaSquared - 1.0) + 1.0;
    return alphaSquared / max(PI * denominator * denominator, 1.0e-7);
}

float SmithGgxPart(float normalDirection, float alpha)
{
    const float alphaSquared = alpha * alpha;
    const float cosineSquared = normalDirection * normalDirection;
    return (2.0 * normalDirection) /
        max(normalDirection + sqrt(alphaSquared + (1.0 - alphaSquared) * cosineSquared), 1.0e-6);
}

float GeometrySmithGgx(float normalView, float normalLight, float alpha)
{
    return SmithGgxPart(normalView, alpha) * SmithGgxPart(normalLight, alpha);
}

float3 ToneMapAces(float3 color)
{
    const float3 numerator = color * (2.51 * color + 0.03);
    const float3 denominator = color * (2.43 * color + 0.59) + 0.14;
    return saturate(numerator / max(denominator, 1.0e-5));
}

float3 LinearToSrgb(float3 color)
{
    const float3 low = color * 12.92;
    const float3 high = 1.055 * pow(max(color, 0.0), 1.0 / 2.4) - 0.055;
    return lerp(high, low, color <= 0.0031308);
}

float DirectionalShadowVisibility(float4 lightPosition, float normalLight)
{
    if (lightPosition.w <= 0.0)
    {
        return 1.0;
    }
    const float3 lightNdc = lightPosition.xyz / lightPosition.w;
    if (lightNdc.x < -1.0 || lightNdc.x > 1.0 ||
        lightNdc.y < -1.0 || lightNdc.y > 1.0 ||
        lightNdc.z < 0.0 || lightNdc.z > 1.0)
    {
        return 1.0;
    }
    const float2 shadowUv = float2(lightNdc.x * 0.5 + 0.5, 0.5 - lightNdc.y * 0.5);
    const float texel = materialOptions.z;
    const float2 halfTexel = texel * 0.5;
    const float bias = max(0.00035 * (1.0 - normalLight), 0.00008);
    float visibility = 0.0;
    [unroll]
    for (int y = -1; y <= 1; ++y)
    {
        [unroll]
        for (int x = -1; x <= 1; ++x)
        {
            const float2 tapUv = clamp(
                shadowUv + float2((float)x, (float)y) * texel,
                halfTexel,
                1.0 - halfTexel);
            const float storedDepth = directionalShadowTexture.SampleLevel(
                materialSampler,
                tapUv,
                0.0);
            visibility += (lightNdc.z - bias <= storedDepth) ? 1.0 : 0.0;
        }
    }
    return visibility / 9.0;
}

float4 PSMain(VertexOutput input) : SV_Target0
{
    // The sRGB view decodes color texels before filtering/sampling. The factor
    // is authored and multiplied in linear light, as required by glTF.
    const float4 baseSample = baseColorTexture.Sample(materialSampler, input.texCoord);
    const float3 baseColor = max(baseSample.rgb * baseColorFactor.rgb * input.instanceTint, 0.0);
    const float4 metallicRoughness = metallicRoughnessTexture.Sample(materialSampler, input.texCoord);
    const float metallic = saturate(materialFactors.x * metallicRoughness.b);
    const float perceptualRoughness = clamp(
        materialFactors.y * metallicRoughness.g,
        0.045,
        1.0);
    const float alpha = perceptualRoughness * perceptualRoughness;

    const float4 encodedNormal = normalTexture.Sample(materialSampler, input.texCoord);
    float3 tangentNormal = encodedNormal.xyz * 2.0 - 1.0;
    tangentNormal.xy *= materialOptions.x;
    tangentNormal = normalize(tangentNormal);

    const float3 normal = normalize(input.worldNormal);
    const float3 tangent = normalize(input.worldTangent.xyz - normal * dot(normal, input.worldTangent.xyz));
    const float3 bitangent = normalize(cross(normal, tangent)) * input.worldTangent.w;
    const float3 shadingNormal = normalize(
        tangent * tangentNormal.x + bitangent * tangentNormal.y + normal * tangentNormal.z);

    const float3 view = normalize(cameraPosition.xyz - input.worldPosition);
    const float3 light = normalize(float3(-0.45, 0.72, -0.52));
    const float3 halfVector = normalize(view + light);
    const float normalView = saturate(dot(shadingNormal, view));
    const float normalLight = saturate(dot(shadingNormal, light));
    const float normalHalf = saturate(dot(shadingNormal, halfVector));
    const float viewHalf = saturate(dot(view, halfVector));

    const float3 f0 = lerp(0.04.xxx, baseColor, metallic);
    const float3 fresnel = FresnelSchlick(viewHalf, f0);
    const float distribution = DistributionGgx(normalHalf, alpha);
    const float geometry = GeometrySmithGgx(normalView, normalLight, alpha);
    const float3 specular = (distribution * geometry * fresnel) /
        max(4.0 * normalView * normalLight, 1.0e-5);
    const float3 diffuseWeight = (1.0 - fresnel) * (1.0 - metallic);
    const float3 diffuse = diffuseWeight * baseColor / PI;

    // One directional key is evaluated by a Cook-Torrance GGX BRDF. A low,
    // hemispherical diffuse fill keeps the prototype legible without claiming
    // to be image-based lighting; ambient remains unshadowed in this first pass.
    const float3 keyRadiance = float3(3.8, 3.55, 3.2);
    const float shadowVisibility = DirectionalShadowVisibility(input.lightPosition, normalLight);
    const float3 keyLight = (diffuse + specular) * keyRadiance * normalLight * shadowVisibility;
    const float hemisphere = saturate(shadingNormal.y * 0.5 + 0.5);
    const float3 ambientIrradiance = lerp(
        float3(0.035, 0.045, 0.065),
        float3(0.19, 0.22, 0.28),
        hemisphere);
    const float3 ambientDiffuse = diffuseWeight * baseColor * ambientIrradiance / PI;
    const float3 radiance = max((keyLight + ambientDiffuse) * materialOptions.y, 0.0);
    const float3 displayColor = LinearToSrgb(ToneMapAces(radiance));
    return float4(displayColor, 1.0);
}
