# Tales of Berseria Remastered — Ultrawide Fix

Corrige a imagem esticada e o HUD em resoluções diferentes de 16:9/16:10 (ex.: 3440x1440).

O jogo roda o motor original (`TLSampleViewerTO12.dll`) dentro do Unity. Esse motor só
conhece 16:9 e 16:10 e "encaixa" qualquer outra proporção em 16:9, o que estica tudo. O mod
corrige isso na memória quando a DLL do motor é carregada:

- **Imagem 3D**: usa a proporção real da resolução (sem esticar).
- **HUD/menus**: o canvas da UI (1280x720 centrado) passa a ter `720·aspecto × 720`,
  mantendo o HUD proporcional e centralizado na área 16:9.
- **Fades/fundos de menu em tela cheia**: redimensionados para cobrir a tela inteira.

As alterações são localizadas por assinatura de bytes; se uma atualização do jogo quebrar
alguma, o mod não aplica aquela parte e registra isso no log.

## Build

```
cargo build --release
```

Gera `target/release/version.dll` (Rust, toolchain MSVC).

## Instalação

1. Copie `version.dll` para a pasta do jogo (`...\Tales of Berseria Remastered\`).
2. No jogo, use a resolução nativa (ex.: 3440x1440).
3. Mantenha desativado o plugin antigo `TalesOfBerseriaUI.addon` (ele aplica correções semelhantes).

Após abrir o jogo, `TOBUltrawide.log` na mesma pasta mostra o que foi aplicado.

## Remoção

Apague `version.dll` (e `TOBUltrawide.log`) da pasta do jogo.
