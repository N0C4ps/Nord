cbuffer CameraData : register(b0)
{
    column_major float4x4 model;
    column_major float4x4 viewProjection;
    column_major float4x4 normalModel;
    float4 cameraPosition;
    column_major float4x4 lightViewProjection;
};

struct VertexInput
{
    float3 position : POSITION;
    float3 instanceTranslation : INSTANCE_TRANSLATION;
};

struct VertexOutput
{
    float4 position : SV_Position;
};

VertexOutput VSMain(VertexInput input)
{
    VertexOutput output;
    float4 worldPosition = mul(model, float4(input.position, 1.0));
    worldPosition.xyz += input.instanceTranslation;
    output.position = mul(lightViewProjection, worldPosition);
    return output;
}

// Nord's current graphics pipeline contract requires a color attachment.
// Store the same normalized light-space depth in R32_FLOAT while D32_FLOAT
// performs nearest-surface testing; later passes sample this scalar map.
float PSMain(float4 position : SV_Position) : SV_Target0
{
    return position.z;
}
