# Histórico de versões

As notas descrevem o conteúdo de cada build pública do Builders Game.

## [Unreleased]

## [1.0.0-alpha.1] - 2026-11-01

Esta é a primeira build pública do Builders Game. Ela serve como ambiente de
teste para a quarta versão da reescrita da Nord Tech, com foco na integração das
crates Jobs, Clock e Renderer em uma janela 3D jogável. É uma demonstração
técnica, ainda sem o ciclo completo de um jogo voxel.

### Adicionado

- Controlador de jogador ainda abstrato, representado pela câmera do jogador.
- Movimentação da câmera com **WASD** ou **setas**.
- Câmera em primeira pessoa: clique na janela para capturar o mouse e pressione
  `Esc` para liberar o cursor.
- Cena de demonstração com cubos instanciados em grade e inclinados na diagonal
  para exercitar a renderização 3D.
- Integração inicial das crates Jobs, Clock e Renderer da Nord Tech.

### Problemas conhecidos e pendências

- Ainda não há conteúdo ou ciclo de jogo completo; esta build se concentra na
  renderização da cena de demonstração e na movimentação da câmera.
- Ainda não há interação voxel para quebrar ou colocar blocos, crosshair ou
  hotbar.
- Esta build tem como alvo Windows. Linux e macOS não são plataformas suportadas
  nem testadas para execução do jogo.
- Nenhum bug conhecido permaneceu ao preparar esta build, mas ainda podem
  existir falhas que não foram identificadas.

### Requisitos

- Windows x64 e GPU/driver compatível com Direct3D 12.
