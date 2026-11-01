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

A crate `nord_dependency` só pode ser referenciada por `crates/nord_app`, que mantém a API estável de serviços usada pela aplicação. `src/` não conhece a crate de DI nem os tipos do container; alterações internas ou incompatíveis no DI ficam isoladas em `crates/nord_app`.

Da mesma forma, `nord_platform` fornece capacidades nativas genéricas, enquanto `nord_app` mantém a fachada de plataforma usada pelo jogo. `src/` integra `ApplicationPlatform` via os serviços da aplicação e não importa `nord_platform` diretamente. O escopo e as capacidades atuais estão em [platform-abstraction.md](platform-abstraction.md).

`nord_log` fornece a infraestrutura de eventos estruturados e sinks assíncronos; a fachada `nord_app` configura e expõe os macros e serviços de logging ao jogo. `src/` não depende diretamente de `nord_log`. A configuração e as garantias de fila estão em [logging.md](logging.md).

`nord_crash` cuida do panic hook, relatórios locais e minidumps nativos fora de
processo. `nord_app` expõe o reporter e coordena um flush limitado do logger
antes da gravação do panic report. Consulte [crash-diagnostics.md](crash-diagnostics.md).

`nord_diagnostics` fornece contexto para cadeias de erro e assertions de
invariantes. `nord_app` mantém a fachada estável; `src/` usa `Result` com
contexto para falhas recuperáveis e assertions para invariantes do próprio
programa. As bibliotecas mantêm erros concretos em suas APIs. Consulte
[diagnostics.md](diagnostics.md) para as regras e os exemplos.

`nord_memory` fornece orçamentos explícitos, scratch arenas e pools tipados
reutilizáveis. `nord_app` mantém a fachada usada pelo jogo. O alocador global
do processo continua sendo uma escolha do executável; a crate não depende da
API instável de allocator nem substitui o heap global. Consulte
[memory-management.md](memory-management.md).

`nord_hash` fornece digests BLAKE3 estáveis, hashing incremental de arquivos e
builders estruturados com domínio e campos tipados para conteúdo e cache keys.
`nord_app` mantém a fachada para o jogo; `HashMap` continua usando seu hasher
próprio para lookup em memória. Consulte [hash-utilities.md](hash-utilities.md).

`nord_math` reúne primitivas de álgebra linear, geometria validada e consultas
espaciais. Crates de engine que precisam de operações matemáticas podem depender
dele; o jogo usa aliases estáveis via `nord_app`. O backend vetorial fica
encapsulado em `nord_math`. Consulte [math-system.md](math-system.md).

`nord_time` fornece clocks monotônicos, timestamps de parede, amostras de FPS,
stopwatch e scheduling de simulação por passo fixo. `nord_app` expõe esses tipos
ao jogo; o runtime alimenta a simulação com o delta limitado e mantém o
interpolador e as métricas de frame. Consulte [time-system.md](time-system.md).

`nord_config` combina defaults tipados, overlays JSON/TOML e variáveis de
ambiente com validação no fim da composição. `nord_app` mantém a fachada, e o
schema com as regras específicas do jogo pertence a `src/app/config.rs`.
Consulte [configuration.md](configuration.md).

`nord_threading` mantém pools de workers explicitamente possuídos; `nord_jobs`
fornece operações paralelas de dados usando o scheduler work-stealing do Rayon;
`nord_scheduler` controla fila limitada, prioridade, cancelamento cooperativo,
handles e shutdown de tarefas maiores. `nord_app` registra os serviços e fornece
a fachada ao jogo, sem expor Rayon em `src/`. Consulte
[threading-and-jobs.md](threading-and-jobs.md).

`src/app/` coordena bootstrap, runtime, lifecycle, shutdown e o limite de
tratamento de erros do jogo. O loop de jogo permanece em `src/gameloop/`; o
estado inicial e os limites do runtime estão em [application-runtime.md](application-runtime.md).

## Evolução e compatibilidade

O objetivo é permitir melhorar ou substituir a implementação interna de uma crate sem reescrever o jogo, desde que sua API pública permaneça compatível. Quando uma mudança de API for realmente necessária, ela deve ser explícita e suas consequências para os consumidores devem ser avaliadas.

Essa separação também reduz o impacto de mudanças: uma crate pode evoluir sem modificar as demais que não dependem dela. Isso não é automático só por haver pastas ou crates separadas; depende de manter responsabilidades, APIs e dependências bem controladas.

## Regra prática

Antes de adicionar código, pergunte:

1. Isso implementa uma capacidade genérica/reutilizável da Nord Tech? Coloque na crate responsável em `crates/`.
2. Isso define conteúdo, regra ou integração específica do jogo? Coloque no módulo apropriado dentro de `src/`.
3. Uma crate precisa conhecer detalhes privados de outra? Reveja a API e a direção da dependência antes de acoplar as duas.

Em resumo: **`crates/` mantém funcionalidades isoladas e reutilizáveis; `src/` conecta essas funcionalidades para formar o jogo.**
