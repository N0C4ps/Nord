cbuffer CameraData : register(b0)
{
    column_major float4x4 model;
    column_major float4x4 viewProjection;
};

struct VertexInput
{
    float3 position : POSITION;
    float3 normal : NORMAL;
};

struct VertexOutput
{
    float4 position : SV_Position;
    float3 worldNormal : NORMAL;
};

VertexOutput VSMain(VertexInput input)
{
    VertexOutput output;
    float4 worldPosition = mul(model, float4(input.position, 1.0));
    output.position = mul(viewProjection, worldPosition);
    output.worldNormal = normalize(mul((float3x3)model, input.normal));
    return output;
}

float4 PSMain(VertexOutput input) : SV_Target0
{
    const float3 lightDirection = normalize(float3(-0.45, 0.72, -0.52));
    const float diffuse = saturate(dot(normalize(input.worldNormal), lightDirection));
    const float3 baseColor = float3(0.12, 0.66, 0.88);
    return float4(baseColor * (0.22 + 0.78 * diffuse), 1.0);
}
