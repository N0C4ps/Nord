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
    return lerp(high, low, 1.0 - step(0.04045, value));
}

float3 rgb_to_hsv(float3 color)
{
    float maximum = max(color.r, max(color.g, color.b));
    float minimum = min(color.r, min(color.g, color.b));
    float delta = maximum - minimum;
    float hue = 0.0;
    if (delta > 0.0001)
    {
        if (maximum == color.r)
            hue = frac((color.g - color.b) / delta / 6.0);
        else if (maximum == color.g)
            hue = ((color.b - color.r) / delta + 2.0) / 6.0;
        else
            hue = ((color.r - color.g) / delta + 4.0) / 6.0;
    }
    return float3(hue, maximum > 0.0001 ? delta / maximum : 0.0, maximum);
}

float3 hsv_to_rgb(float3 hsv)
{
    float3 hue = abs(frac(hsv.xxx + float3(0.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0);
    return hsv.z * lerp(1.0, saturate(hue - 1.0), hsv.y);
}

float3 vivid_inversion(float3 background)
{
    // Invert this exact scene pixel first, then boost its own saturation and
    // gently lift midtones. No neighborhood or screen-wide color choice means
    // an isolated colored pixel cannot recolor the rest of the crosshair.
    float3 hsv = rgb_to_hsv(1.0 - background);
    hsv.y = saturate(hsv.y * 1.35);
    hsv.z = saturate((hsv.z - 0.5) * 1.25 + 0.58);
    return hsv_to_rgb(hsv);
}

float4 PSCrosshair(VertexOutput input) : SV_Target0
{
    float3 background = SceneColor.Load(int3(int2(input.position.xy), 0)).rgb;
    return float4(vivid_inversion(background), 1.0);
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
