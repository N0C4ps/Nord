struct FrameData
{
    float4 tint;
};

ConstantBuffer<FrameData> frameData[2] : register(b0);

Texture2D<float4> sourceTextures[2] : register(t0);
SamplerState sourceSamplers[2] : register(s0);

struct VertexOutput
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
};

struct VertexInput
{
    float3 position : POSITION;
    float2 uv : TEXCOORD0;
};

VertexOutput VSMain(VertexInput input)
{
    VertexOutput output;
    output.position = float4(input.position, 1.0);
    output.uv = input.uv;
    return output;
}

struct PixelOutput
{
    float4 color : SV_Target0;
    float4 auxiliary : SV_Target1;
};

PixelOutput PSMain(VertexOutput input)
{
    PixelOutput output;
    output.color = sourceTextures[0].Sample(sourceSamplers[0], input.uv) * frameData[0].tint;
    output.auxiliary = float4(0.0, 1.0, 0.0, 1.0);
    return output;
}

cbuffer ComputeData : register(b1)
{
    float4 computeTint;
};

RWTexture2D<float4> outputTexture : register(u0);
RWTexture2D<float4> outputTextures[2] : register(u1);

[numthreads(1, 1, 1)]
void CSMain(uint3 threadId : SV_DispatchThreadID)
{
    outputTexture[threadId.xy] = computeTint;
}

[numthreads(1, 1, 1)]
void CSArrayMain(uint3 threadId : SV_DispatchThreadID)
{
    outputTextures[0][threadId.xy] = computeTint;
    outputTextures[1][threadId.xy] = computeTint;
}
