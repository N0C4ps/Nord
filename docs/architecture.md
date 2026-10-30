# Arquitetura do Nord

## Ideia central

O repositório tem duas camadas com responsabilidades diferentes:

- `crates/` contém a Nord Tech: bibliotecas reutilizáveis, cada uma responsável por uma funcionalidade ou infraestrutura bem delimitada.
- `src/` contém o jogo Nord: código que conecta e configura essas bibliotecas para criar o jogo.

As crates fornecem capacidades. O jogo decide como essas capacidades são combinadas e usadas.

## Crates isoladas

Cada crate deve ter uma responsabilidade própria, uma API pública clara e implementação interna encapsulada. Por exemplo, `nord_render` oferece infraestrutura de renderização; `nord_log` oferece logging; e `nord_physics` oferece infraestrutura de física.

Uma crate não deve depender dos detalhes internos de outra. A dependência deve existir apenas quando houver uma necessidade arquitetural real, e deve passar por uma API pública estável e pequena. Em particular, mudar `nord_log` não deve exigir mudanças em `nord_physics`, e melhorias internas em uma crate não devem afetar consumidores que usam sua API sem alterações.

As crates da Nord Tech não devem conter regras específicas do jogo Nord. Elas devem continuar úteis e compreensíveis como bibliotecas independentes.

## O jogo conecta as funcionalidades

O código de `src/` é o lugar para as decisões específicas do jogo e para integrar os sistemas. Por exemplo:

- `crates/nord_render` implementa a infraestrutura de renderização; `src/render/` configura e utiliza essa infraestrutura para o Nord.
- `crates/nord_physics` implementa capacidades genéricas de física; `src/physics/` define como o jogo as integra às suas regras.
- `crates/nord_world` oferece estruturas genéricas para mundos; `src/world/` define como o Nord compõe e usa seu mundo.

O fluxo de dependência deve apontar para as APIs das bibliotecas: o jogo conhece e usa as crates. As crates não devem conhecer o jogo nem importar suas regras.

`nord_dependency` fornece o registro tipado para a composição inicial das funcionalidades. O registro fica na raiz de composição (`src/app/`); sistemas recebem os serviços necessários por parâmetros de construção. O container não deve se tornar um singleton global nem ser passado a cada sistema para buscas arbitrárias, porque isso esconderia as dependências reais dos módulos.

## Evolução e compatibilidade

O objetivo é permitir melhorar ou substituir a implementação interna de uma crate sem reescrever o jogo, desde que sua API pública permaneça compatível. Quando uma mudança de API for realmente necessária, ela deve ser explícita e suas consequências para os consumidores devem ser avaliadas.

Essa separação também reduz o impacto de mudanças: uma crate pode evoluir sem modificar as demais que não dependem dela. Isso não é automático só por haver pastas ou crates separadas; depende de manter responsabilidades, APIs e dependências bem controladas.

## Regra prática

Antes de adicionar código, pergunte:

1. Isso implementa uma capacidade genérica/reutilizável da Nord Tech? Coloque na crate responsável em `crates/`.
2. Isso define conteúdo, regra ou integração específica do jogo? Coloque no módulo apropriado dentro de `src/`.
3. Uma crate precisa conhecer detalhes privados de outra? Reveja a API e a direção da dependência antes de acoplar as duas.

Em resumo: **`crates/` mantém funcionalidades isoladas e reutilizáveis; `src/` conecta essas funcionalidades para formar o jogo.**
