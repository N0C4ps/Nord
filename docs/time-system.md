# Nord time system

## Relógios distintos para problemas distintos

`nord_time` separa duas noções de tempo:

- `FrameClock` e `Stopwatch` usam `Instant`, que é monotônico e serve para
  durações, frame delta e profiling. Não use relógio de parede para física ou
  tempo decorrido.
- `UnixTimestamp` usa `SystemTime` e representa nanosegundos desde a época Unix.
  Use para arquivos salvos, logs e correlação externa; o relógio do sistema pode
  ser corrigido e não serve para medir intervalos.

## Frame timing e estabilidade

`FrameClock` retorna o delta bruto e o delta de jogo. Por padrão, limita um
delta de frame a 250 ms para evitar que uma pausa, breakpoint ou suspensão
cause um salto enorme na simulação. O tempo bruto permanece disponível para
telemetria e para o contador de FPS. O runtime compartilha um único `FrameTime`
entre os sistemas de um frame. Um `time_scale` de zero pausa o tempo de jogo,
um mantém a escala real e valores entre 0 e 100 permitem slow motion ou fast
forward; o delta não escalado continua separado para medir FPS.

`FixedStepAccumulator` agenda atualizações determinísticas em intervalos
constantes, por padrão aproximadamente 60 Hz, independentemente da taxa de
renderização. O limite de oito catch-up steps por frame evita trabalho sem fim
após uma pausa longa; passos inteiros excedentes são descartados e registrados,
e o restante fracionário vira `interpolation_alpha` para interpolar o render.
Mundos com autoridade de rede, replay ou física exigem revisar o step e a
política de catch-up antes de fixar essas configurações.

`FpsCounter` calcula uma média em janelas de um segundo sem alocação por frame.
Ele fornece FPS e tempo médio de frame, sem escrever um log a cada iteração.
Percentis de frame time e histogramas podem ser adicionados à telemetria quando
houver uma camada de profiling definida.

## Uso no jogo

`src/gameloop/` obtém `ApplicationFrameTime` em cada iteração e entrega o delta
de jogo ao acumulador de passo fixo. Guarda também passos de simulação
agendados, backlog descartado e a última amostra de FPS; o evento único de
encerramento inclui essas métricas. As APIs públicas chegam ao jogo por
`nord_app`.

```rust
let frame = clock.tick();
let plan = fixed_step.advance(frame.delta);
for _ in 0..plan.steps {
    simulate_fixed_step(fixed_step_config.step());
}
render_interpolated(plan.interpolation_alpha);
```

## Referências

- [`std::time::Instant`](https://doc.rust-lang.org/std/time/struct.Instant.html)
  documenta um relógio monotônico para medições de duração.
- [`std::time::SystemTime`](https://doc.rust-lang.org/std/time/struct.SystemTime.html)
  documenta o relógio de parede e seus ajustes não monotônicos.
- [Bevy Time](https://docs.rs/bevy_time/latest/bevy_time/) separa recursos de
  tempo real, virtual, fixed e time scale na arquitetura de engine.
- [Bevy `FixedUpdate`](https://docs.rs/bevy_app/latest/bevy_app/struct.FixedUpdate.html)
  descreve a agenda de simulação por passos fixos.
