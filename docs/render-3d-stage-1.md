# Renderer 3D — Etapa 1

Esta etapa estabelece o primeiro caminho 3D executado pelo jogo: câmera
perspectiva, transformação de objeto, malha indexada, attachment de profundidade
e apresentação pela infraestrutura gráfica da Nord.

## Responsabilidades

- `nord_camera` contém pose e projeção perspectiva independentes de janela,
  input e API gráfica. Usa câmera RH, matrizes coluna-major e profundidade de
  clip `[0, 1]`, a convenção usada pelo caminho D3D12 atual.
- `nord_mesh` valida vértices, índices de triangle-list e bounds locais. Ele
  não cria buffers nem conhece shaders ou o renderer.
- `nord_render` continua responsável por recursos GPU, pipeline, command
  encoder, render graph, depth attachment e apresentação.
- `src/render` escolhe a câmera, cria o cubo de demonstração, atualiza seus
  dados por frame e registra o passe do jogo.
- `src/render/material.rs` define o contrato metallic-roughness do jogo, carrega
  os shaders embutidos, cria mapas GPU e valida os bindings refletidos.
  `nord_render` continua oferecendo apenas compilação, reflexão, pipeline,
  buffers, texturas, samplers e gravação de comandos genéricos.
- `src/camera.rs` conecta os eventos normalizados de `nord_inputs` à pose de
  `nord_camera`; as duas crates continuam sem dependência direta entre si.

## Caminho do frame

1. A câmera calcula `projection * view`; o jogo combina isso com `model`.
2. O jogo envia os vértices e índices da malha para buffers device-local.
3. Um buffer de constantes upload, alinhado a 256 bytes, recebe model,
   view-projection, normal matrix, posição da câmera e matriz da luz em `b0`.
   O material envia fatores metallic-roughness em `b1` e liga base color,
   metallic-roughness, normal, mapa de sombras e sampler nos slots
   `t0..t3/s0`.
4. O render graph declara o passe da sombra direcional antes do passe PBR. O
   primeiro usa o frustum da luz e escreve os alvos da sombra; o segundo lê a
   textura de profundidade, além dos três mapas de material.
5. Os passes limpam seus alvos, executam draws indexados instanciados e o jogo
   apresenta a textura colorida pela swapchain.
6. Ao mudar o tamanho da janela, o jogo recria os attachments e atualiza o
   aspect ratio da câmera. Uma superfície minimizada não é renderizada.

## Material metallic-roughness PBR

`PbrMaterialDesc` recebe `base_color_linear`, `metallic_factor`,
`roughness_factor`, `normal_scale` e `exposure`, validando valores finitos e
seus intervalos. O jogo combina fatores e mapas como no glTF: base color RGB é
armazenado como sRGB e decodificado pelo SRV sRGB antes da multiplicação linear;
metallic-roughness fica em UNORM linear, roughness no canal G e metallic no B;
o normal map fica em UNORM linear, é remapeado de `[0,1]` para tangente
`[−1,1]`, escalado e aplicado por TBN. Os mapas da demonstração são gerados
proceduralmente no jogo, recebem mip chains com filtragem correta em espaço
linear para base color e renormalização de normais, e não dependem de arquivos
externos.

O pixel shader usa Lambert para diffuse e Cook-Torrance com GGX, Smith e Fresnel
de Schlick para a resposta especular. `roughness` perceptual é elevada ao
quadrado para formar `alpha`; metallic reduz a parcela difusa e colore `F0`,
com `0.04` para dielétricos. A iluminação inicial tem uma luz direcional e um
preenchimento difuso hemisférico baixo; isso mantém o protótipo legível, mas
ainda não é IBL, ambiente HDR, múltiplas luzes ou iluminação de produção.
Sombras da luz direcional são descritas adiante. Radiância linear passa por
tone mapping ACES e conversão linear→sRGB no pixel shader porque o alvo atual
é `Rgba8Unorm` e a apresentação copia os bytes ao backbuffer.

O pipeline continua opaco, com blending desligado, profundidade `D32Float` e
sampler anisotrópico repeat. `assets/shaders/nord_pbr_forward.hlsl` é asset do
jogo; o `build.rs` compila VS/PS para DXIL e incorpora bytecode e reflexão no
executável. O runtime valida bindings de câmera, material, texturas e sampler;
o jogador não precisa da pasta de shaders nem de DXC.

O foco da janela, sozinho, não captura o mouse. Um clique primário dentro da
janela focada ativa o modo de primeira pessoa: o backend Win32 esconde e mantém
o cursor no centro da área cliente, restringe-o à janela e entrega movimento
relativo via Raw Input. `Esc` restaura o cursor à posição anterior; perder foco
ou destruir a janela também libera captura. `src/camera.rs` decide quando
capturar e converte deltas em rotação, sem dependência direta entre
`nord_inputs`, `nord_camera` e `nord_render`. O pitch é limitado antes da
singularidade vertical e a rotação independe do delta time do frame.

O shader recebe a matriz normal inversa-transposta calculada a partir do
transform do objeto; tangentes passam pela parte linear do modelo e são
ortogonalizadas contra a normal no vertex shader. O cubo de demonstração usa
escala positiva. Transformações espelhadas ainda precisam inverter a
handedness da tangente antes de serem usadas por uma malha geral.

## Culling e instancing pela CPU

`src/render/visibility.rs` cria uma grade de demonstração com 1.681 instâncias
estáticas da mesma malha/material. O jogo extrai os seis planos do frustum a
partir da matriz `projection * view` usando a convenção D3D de profundidade
`[0, 1]`, transforma os oito cantos do AABB local para obter bounds mundiais e
faz o teste conservador de AABB contra os planos. Bounds inválidos ou uma
matriz que não permita extrair um frustum válido falham abertos e mantêm a
instância visível; bounds válidos recebem uma pequena margem contra erro
numérico.

Os dados de instância (translação e tint linear) são enviados uma vez para um
buffer de vértices device-local. A cada frame a CPU atualiza uma máscara de
visibilidade e gera ranges contíguos ordenados por mesh/material. O command
encoder envia draws indexados instanciados com `first_instance` e
`instance_count`; o shader aplica translação e tint por instância. Como o
buffer é imutável durante o runtime, a atualização da câmera não regrava dados
que a GPU possa estar lendo.

O evento estruturado `cpu_visibility_instancing_metrics` registra a medição da
câmera inicial uma vez, sem produzir log por frame. `total_instances`,
`visible_instances` e `culled_instances` mostram o trabalho de culling;
`naive_draw_calls` estima um draw por objeto visível e
`instanced_draw_calls` conta os ranges realmente submetidos. `culling_ns` e
`grouping_ns` são tempos CPU medidos com `Instant`. O script
`scripts/verify_single_file_release.ps1` inicia o release isolado e imprime esse
evento do log para provar que o caminho chegou ao runtime. Esses tempos medem
somente culling e agrupamento na CPU, não o frame completo nem o tempo da GPU.
Na execução de verificação local em Direct3D 12, a câmera inicial manteve 573
das 1.681 instâncias e rejeitou 1.108; o envio estimado caiu de 573 draws
individuais para 24 ranges instanciados (95,8% menos submissões), com 22,3 µs
de culling e 1,7 µs de agrupamento nessa amostra. É uma medição pontual de
inicialização, útil para confirmar o comportamento e comparar futuras mudanças,
não um benchmark estatístico de performance.

Este é um caso controlado com uma malha e um material. Os identificadores de
mesh/material já fazem parte da chave de batch e têm teste cobrindo múltiplos
grupos; integração posterior de meshes e materiais deve fornecer seus próprios
buffers/pipelines antes de habilitar essas chaves na cena. Não há occlusion
culling nesta etapa.

## Sombras direcionais

O jogo registra dois passes no render graph por frame:

1. `Nord directional shadow depth` renderiza a cena pela matriz ortográfica da
   luz, usando o mesmo mesh indexado e o buffer de instâncias. O D32 faz o
   teste da superfície mais próxima; o passe também grava a profundidade
   normalizada em um alvo R32 amostrável.
2. `Nord metallic-roughness PBR mesh` declara leitura amostrável desse mapa e
   aplica uma comparação PCF 3×3 ao termo de luz direcional. A luz ambiente
   permanece sem sombras nesta etapa.

A visibilidade do passe de sombra é calculada com o frustum da luz, separado do
frustum da câmera. O mapa tem 2048² texels e uma projeção ortográfica que cobre
a cena de demonstração; coordenadas fora do mapa são consideradas iluminadas.
O shader aplica viés de profundidade dependente do ângulo e limita cada toque
do PCF à borda útil do mapa. D32 e R32 são alvos distintos porque o contrato
atual da pipeline gráfica requer um alvo de cor; o R32 funciona como a textura
escalar de profundidade lida pelo passe PBR.

Esta etapa cobre a cena de demonstração com uma única luz direcional. Cascatas,
ajuste do mapa a limites dinâmicos do mundo, textura de profundidade amostrável
nativa e bias por derivadas são evoluções para mundos maiores, câmeras distantes
e maior qualidade de sombra.

## Referências técnicas

- Microsoft, [Direct3D transformation pipeline](https://learn.microsoft.com/en-us/windows/win32/dxtecharts/the-direct3d-transformation-pipeline): transformações world/view/projection, clip space homogêneo e viewport.
- Microsoft, [`DrawIndexedInstanced`](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12graphicscommandlist-drawindexedinstanced): parâmetros do draw indexado e leitura do index buffer.
- Microsoft, [`D3D12_INPUT_ELEMENT_DESC`](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/ns-d3d12-d3d12_input_element_desc): classificação de dados por vértice ou por instância e step rate do input assembler.
- Microsoft, [`CreateDepthStencilView`](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device-createdepthstencilview): criação do DSV do recurso depth.
- Microsoft, [D3D12 root signatures](https://learn.microsoft.com/en-us/windows/win32/direct3d12/creating-root-signature): descriptors e bindings refletidos pelo shader.
- Microsoft, [Managing Graphics Pipeline State in Direct3D 12](https://learn.microsoft.com/en-us/windows/win32/direct3d12/managing-graphics-pipeline-state-in-direct3d-12): blend, rasterizer, depth/stencil e shaders como estado imutável do pipeline.
- Microsoft, [`D3D12_DEPTH_STENCIL_DESC`](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/ns-d3d12-d3d12_depth_stencil_desc): ativação do teste, escrita e comparação de profundidade.
- Microsoft, [Render the shadow map to the depth buffer](https://learn.microsoft.com/en-us/windows/uwp/gaming/render-the-shadow-map-to-the-depth-buffer): renderizar da perspectiva da luz para produzir o mapa de profundidade de sombra.
- Microsoft, [Render the scene with depth testing](https://learn.microsoft.com/en-us/windows/uwp/gaming/render-the-scene-with-depth-testing): comparar a profundidade em espaço da luz com o mapa de sombras durante o passe de cena.
- Microsoft, [Cascaded Shadow Maps](https://learn.microsoft.com/en-us/windows/desktop/DxTechArts/cascaded-shadow-maps): PCF, amostragem da vizinhança e tradeoffs de depth bias.
- Microsoft, [Shader Constants (HLSL)](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-constants): layout e bindings dos constant buffers usados por câmera e material.
- Khronos, [glTF 2.0 — Metallic-Roughness Material](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#metallic-roughness-material): fatores lineares, transferência sRGB da base color e canais G/B do mapa metallic-roughness.
- Khronos, [glTF 2.0 — BRDF implementation](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#appendix-b-brdf-implementation): GGX, Fresnel, diffuse/specular e convenções de roughness.
- Microsoft, [Raw Input overview](https://learn.microsoft.com/en-us/windows/win32/inputdev/raw-input) e [Using Raw Input](https://learn.microsoft.com/en-us/windows/win32/inputdev/using-raw-input): recebimento de deltas de dispositivo via `WM_INPUT` e leitura com `GetRawInputData`.
- Microsoft, [`ClipCursor`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-clipcursor): confinamento e liberação explícita do cursor ao alternar o modo de captura.
