# Nord hash utilities

## Responsabilidade

`crates/nord_hash` fornece hashes estáveis para conteúdo, arquivos e registros
estruturados. `nord_app` reexporta uma fachada estável; o jogo e as crates
consumidoras não precisam conhecer a implementação BLAKE3 diretamente.

## Escolhas

- **BLAKE3 de 256 bits** para IDs de conteúdo, fingerprints de textura e cache
  keys. O crate oficial oferece atualização incremental, streaming e
  implementações otimizadas para diferentes CPUs.
- **`ContentHasher`** para processar bytes em blocos ou dados vindos de um
  `Read`, sem carregar o arquivo inteiro na memória.
- **`HashBuilder`** para metadados: exige um domínio versionado e codifica cada
  campo com tipo e comprimentos fixos. Assim a concatenação de campos não cria
  entradas ambíguas.
- **`ContentHash`** como tipo de digest de 32 bytes, comparável, ordenável,
  serializável manualmente como hex minúsculo e parseável de volta.
- **`hash_file`** identifica o conteúdo, não o caminho ou metadados do arquivo.
  O chamador inclui dimensões, formato, opções de importação e versão do
  pipeline em campos do builder quando eles alterarem o resultado derivado.

## Exemplo para uma textura

```rust
let digest = nord_app::ApplicationHashBuilder::new("nord.texture.cache.v1")
    .field_u32("width", width)
    .field_u32("height", height)
    .field_str("format", format_name)
    .field_bytes("pixels", pixel_bytes)
    .finish();
```

Inclua todas as opções que afetem o resultado (por exemplo, versão do decoder,
compressor e mipmap settings) no domínio ou nos campos. Para deduplicar somente
os bytes originais do arquivo, use `application_hash_file`.

## O que não usar como content ID

`DefaultHasher` e `HashMap` são destinados a lookup em memória. A documentação
do Rust não garante que o algoritmo interno de `DefaultHasher` ou seus
resultados permaneçam iguais entre versões. O `Hash` da std também não promete
encoding portável entre arquiteturas ou estável entre compiladores. Por isso,
hashes persistentes passam pela API BLAKE3 desta crate, e os mapas continuam
usando seu hasher padrão com randomização para resistência a HashDoS.

BLAKE3 é um hash criptográfico de propósito geral, não uma função de derivação
de senhas. Hashes não substituem validação de formato, autenticação de pacote
ou assinatura de conteúdo.

## Referências

- [Implementação oficial BLAKE3](https://github.com/BLAKE3-team/BLAKE3) e
  [`blake3::Hasher`](https://docs.rs/blake3/latest/blake3/struct.Hasher.html)
  documentam hashing incremental, streaming e suporte otimizado.
- [`DefaultHasher`](https://doc.rust-lang.org/std/collections/hash_map/struct.DefaultHasher.html)
  não garante estabilidade do algoritmo ou do resultado entre releases.
- [`Hash`](https://doc.rust-lang.org/std/hash/trait.Hash.html) documenta as
  limitações de portabilidade e estabilidade dos bytes alimentados aos hashers.
