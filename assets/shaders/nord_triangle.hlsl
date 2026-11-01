struct VertexInput
{
    float3 position : POSITION;
};

struct VertexOutput
{
    float4 position : SV_Position;
};

VertexOutput VSMain(VertexInput input)
{
    VertexOutput output;
    output.position = float4(input.position, 1.0);
    return output;
}

float4 PSMain() : SV_Target0
{
    return float4(0.12, 0.78, 1.0, 1.0);
}
