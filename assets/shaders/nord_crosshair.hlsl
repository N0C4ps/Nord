Texture2D<float4> SceneColor : register(t0);

struct VertexInput
{
    float2 position : POSITION;
};

struct VertexOutput
{
    float4 position : SV_Position;
};

VertexOutput VSMain(VertexInput input)
{
    VertexOutput output;
    output.position = float4(input.position, 0.0, 1.0);
    return output;
}

float3 srgb_to_linear(float3 value)
{
    float3 low = value / 12.92;
    float3 high = pow(max((value + 0.055) / 1.055, 0.0), 2.4);
    float3 use_low = 1.0 - step(0.04045, value);
    return lerp(high, low, use_low);
}

float3 linear_to_srgb(float3 value)
{
    float3 low = value * 12.92;
    float3 high = 1.055 * pow(max(value, 0.0), 1.0 / 2.4) - 0.055;
    float3 use_low = 1.0 - step(0.0031308, value);
    return lerp(high, low, use_low);
}

float4 PSCrosshair(VertexOutput input) : SV_Target0
{
    float3 background_srgb = SceneColor.Load(int3(int2(input.position.xy), 0)).rgb;
    float3 background_linear = srgb_to_linear(background_srgb);
    float luminance = dot(background_linear, float3(0.2126, 0.7152, 0.0722));
    // Measure neutrality before linearization: linear-light channel spread
    // shrinks on dark saturated colors and incorrectly classifies dark wood
    // as gray. Keep the luminance and complementary transform linear below.
    float chroma = max(background_srgb.r, max(background_srgb.g, background_srgb.b)) -
                  min(background_srgb.r, min(background_srgb.g, background_srgb.b));

    // Neutral surfaces have no useful complementary hue. Use a clear neutral
    // instead; this intentionally makes middle gray white, like Minecraft.
    if (chroma < 0.08)
    {
        float neutral = luminance < 0.74 ? 1.0 : 0.0;
        return float4(neutral, neutral, neutral, 1.0);
    }

    // Complement in linear light, then encode back to the UNORM output. This
    // keeps blue sky gold and green surfaces magenta while avoiding dim RGB
    // complements caused by inverting gamma-encoded bytes directly.
    float3 complement = 1.0 - background_linear;
    float complement_luminance = dot(complement, float3(0.2126, 0.7152, 0.0722));
    float contrast = (max(luminance, complement_luminance) + 0.05) /
                     (min(luminance, complement_luminance) + 0.05);
    if (contrast < 4.5)
    {
        // Keep the complement's hue while moving its luminance far enough
        // away from the sampled surface to preserve readable contrast.
        float background_is_light = step(0.18, luminance);
        float target_luminance = lerp(
            4.5 * (luminance + 0.05) - 0.05,
            (luminance + 0.05) / 4.5 - 0.05,
            background_is_light
        );
        target_luminance = saturate(target_luminance);
        if (target_luminance < complement_luminance)
        {
            complement *= target_luminance / max(complement_luminance, 0.0001);
        }
        else
        {
            float lift = (target_luminance - complement_luminance) /
                         max(1.0 - complement_luminance, 0.0001);
            complement = lerp(complement, 1.0, saturate(lift));
        }
    }
    return float4(linear_to_srgb(complement), 1.0);
}

float4 VSBlit(uint vertex_id : SV_VertexID) : SV_Position
{
    float2 uv = float2((vertex_id << 1) & 2, vertex_id & 2);
    return float4(uv * float2(2.0, -2.0) + float2(-1.0, 1.0), 0.0, 1.0);
}

float4 PSBlit(float4 position : SV_Position) : SV_Target0
{
    return SceneColor.Load(int3(int2(position.xy), 0));
}
