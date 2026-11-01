# Nord memory management

## Responsabilidade e integração

`crates/nord_memory` fornece políticas explícitas para memória gerenciada pela
engine. `nord_app` expõe `ApplicationMemoryManager`; `src/` recebe esse serviço
na composição da aplicação. Outras crates não dependem diretamente de
`nord_memory` e continuam livres para escolher estruturas de dados adequadas.

## Recursos atuais

- **Memory budgets:** reservas concorrentes com limite opcional, pico observado
  e tokens RAII que liberam a contagem quando caem fora de escopo. São
  contabilização cooperativa: o chamador reserva antes da alocação e mantém o
  token enquanto possuir aquela memória.
- **Scratch arenas:** alocações temporárias para frame, job ou fase de loading,
  com limite de bytes de payload, cópia de slices e strings, contadores e reset
  em lote. Crie uma arena por worker ou fase; ela não é sincronizada.
- **Object pools:** slots tipados reutilizáveis com handles generacionais. A
  remoção invalida o handle antigo; em caso de wrap de geração, o slot é
  aposentado para impedir que um handle obsoleto volte a ficar válido.
- **Falhas explícitas:** APIs de reserva e inserção usam `Result`/`try_reserve`
  para permitir que o chamador trate pressão de memória sem panic por
  crescimento do armazenamento.

## Regras de uso

1. Use `MemoryManager::reserve` para contabilizar alocações cujo ciclo de vida
   o sistema proprietário consegue acompanhar. Guarde o `MemoryReservation`
   junto do recurso e redimensione o token quando a reserva mudar.
2. Use `ScratchArena` quando todos os valores puderem ser descartados juntos.
   Reset e destruição da arena não chamam `Drop` nos valores nela alocados;
   não use para valores que possuem recursos que exigem destruição.
3. Use `ObjectPool<T>` para objetos pequenos de vida irregular e churn elevado.
   Pré-reserve slots antes de trechos sensíveis a latência e não compartilhe o
   pool entre threads sem sincronização externa.
4. Separe orçamentos por subsistema e defina limites a partir de medições da
   plataforma e do conteúdo. O Nord inicia com o orçamento global sem limite,
   para evitar inventar um teto que ainda não foi medido.

## Alocador global e estabilidade

A crate não troca o alocador global. A API estável `GlobalAlloc` exige uma
implementação `unsafe`, não pode desenrolar panics e precisa evitar recursão
durante alocações. Já o trait `Allocator` para usar allocators em coleções da
stdlib permanece instável. Essas escolhas pertencem ao executável e só devem
ser feitas depois de benchmarking representativo; subsistemas podem começar
com arenas e pools sem afetar todo o processo.

A scratch arena usa `bumpalo`. Como em qualquer arena bump, `reset` é uma
desalocação em lote e não executa `Drop`; o limite da API contabiliza payload,
sem incluir padding de alinhamento ou metadados do allocator. `try_reserve` de
`Vec` e o pool tornam falhas de crescimento observáveis, mas não transformam
todas as alocações infalíveis do Rust em operações recuperáveis.

## Referências

- [`GlobalAlloc` do Rust](https://doc.rust-lang.org/std/alloc/trait.GlobalAlloc.html)
  documenta os contratos `unsafe`, a proibição de unwinding e os cuidados com
  reentrância.
- [`Allocator` do Rust](https://doc.rust-lang.org/std/alloc/trait.Allocator.html)
  está marcado como API instável.
- [`bumpalo::Bump`](https://docs.rs/bumpalo/latest/bumpalo/struct.Bump.html)
  documenta arenas, reset, limites e o fato de valores alocados não receberem
  `Drop` automaticamente.
- [`Vec::try_reserve`](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.try_reserve)
  permite tratar erros de capacidade sem panic.
