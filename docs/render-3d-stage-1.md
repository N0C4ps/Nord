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
  dados por frame, define o shader e registra o passe do jogo.
- `src/camera.rs` conecta os eventos normalizados de `nord_inputs` à pose de
  `nord_camera`; as duas crates continuam sem dependência direta entre si.

## Caminho do frame

1. A câmera calcula `projection * view`; o jogo combina isso com `model`.
2. O jogo envia os vértices e índices da malha para buffers device-local.
3. Um buffer de constantes upload, alinhado a 256 bytes, recebe as duas
   matrizes e fica ligado ao `b0` do shader.
4. O render graph declara os usos de color, depth, vertex, index e constants.
5. O passe limpa color e depth, executa um draw indexado e apresenta a textura
   colorida pela swapchain.
6. Ao mudar o tamanho da janela, o jogo recria os attachments e atualiza o
   aspect ratio da câmera. Uma superfície minimizada não é renderizada.

O foco da janela, sozinho, não captura o mouse. Um clique primário dentro da
janela focada ativa o modo de primeira pessoa: o backend Win32 esconde e mantém
o cursor no centro da área cliente, restringe-o à janela e entrega movimento
relativo via Raw Input. `Esc` restaura o cursor à posição anterior; perder foco
ou destruir a janela também libera captura. `src/camera.rs` decide quando
capturar e converte deltas em rotação, sem dependência direta entre
`nord_inputs`, `nord_camera` e `nord_render`. O pitch é limitado antes da
singularidade vertical e a rotação independe do delta time do frame.

O cubo usa escala uniforme e rotação, portanto transformar normais com a parte
3×3 do modelo é válido nesta demonstração. Escala não uniforme exigirá matriz
normal inversa-transposta antes de virar uma capacidade geral de mesh rendering.

## Referências técnicas

- Microsoft, [Direct3D transformation pipeline](https://learn.microsoft.com/en-us/windows/win32/dxtecharts/the-direct3d-transformation-pipeline): transformações world/view/projection, clip space homogêneo e viewport.
- Microsoft, [`DrawIndexedInstanced`](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12graphicscommandlist-drawindexedinstanced): parâmetros do draw indexado e leitura do index buffer.
- Microsoft, [`CreateDepthStencilView`](https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device-createdepthstencilview): criação do DSV do recurso depth.
- Microsoft, [D3D12 root signatures](https://learn.microsoft.com/en-us/windows/win32/direct3d12/creating-root-signature): descriptors e bindings refletidos pelo shader.
- Microsoft, [Raw Input overview](https://learn.microsoft.com/en-us/windows/win32/inputdev/raw-input) e [Using Raw Input](https://learn.microsoft.com/en-us/windows/win32/inputdev/using-raw-input): recebimento de deltas de dispositivo via `WM_INPUT` e leitura com `GetRawInputData`.
- Microsoft, [`ClipCursor`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-clipcursor): confinamento e liberação explícita do cursor ao alternar o modo de captura.
