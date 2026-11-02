# Publicar releases

O changelog é a fonte das notas públicas: registre em `CHANGELOG.md` o que foi
adicionado, alterado, corrigido e quais problemas continuam conhecidos. As
notas devem descrever o comportamento confirmado na build; não liste planos
como se já estivessem implementados.

## Preparar uma versão

1. Atualize a versão do pacote principal em `Cargo.toml` e em `Cargo.lock`.
   Ela é incluída nos logs e relatórios de crash do executável.
2. Crie uma seção `## [versão] - AAAA-MM-DD` em `CHANGELOG.md` e escreva as notas
   em português claro. Para uma versão alpha, explique o que já funciona e o
   que ainda está ausente.
3. Faça commit e push da preparação para `main`, e espere a validação de CI.
4. No Windows com GPU D3D12, rode `./scripts/verify_single_file_release.ps1`.
   Esse smoke test valida o executável real e requer uma máquina com renderer
   D3D12; por isso fica fora dos runners genéricos do GitHub Actions.
5. Crie e envie uma tag anotada com a mesma versão:

   ```powershell
   $version = '1.0.0-alpha.1'
   git tag -a "v$version" -m "Builders Game $version"
   git push origin "v$version"
   ```

O workflow `.github/workflows/release.yml` é disparado pela tag. Ele confere a
versão, valida e compila o workspace nos três perfis, extrai as notas da seção
correspondente do changelog e publica uma GitHub Release. Builds alpha, beta e
rc são marcadas como pré-lançamento. O workflow anexa `Nord.exe` e seu arquivo
SHA-256.

Não reutilize nem mova uma tag publicada. Corrija o problema em uma nova versão
alpha, como `1.0.0-alpha.2`, para manter downloads e diagnósticos reproduzíveis.
