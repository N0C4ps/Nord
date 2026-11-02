struct VertexInput
{
    float2 position : POSITION;
    float3 color : COLOR0;
};

struct VertexOutput
{
    float4 position : SV_Position;
    float3 color : COLOR0;
};

VertexOutput VSMain(VertexInput input)
{
    VertexOutput output;
    output.position = float4(input.position, 0.0, 1.0);
    output.color = input.color;
    return output;
}

float4 PSMain(VertexOutput input) : SV_Target0
{
    return float4(input.color, 1.0);
}
