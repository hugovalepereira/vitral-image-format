# 02 — Requisitos

Estado: **rascunho v0.1** (out/2026). Os requisitos marcados com 🔵 são da v1;
🟣 ficam para a v2 ou mais tarde.

## Visão

> Uma imagem que transporta, dentro do próprio ficheiro, **como deve reagir ao que
> está por trás dela** — tal como o PNG transporta a transparência.

Ideia de fundo do autor: *cada pixel é uma função do que está atrás dele* (daí o
nome original, *Graphic Function Image*). O Vitral organiza essa ideia em níveis:

| Nível | O pixel depende de… | Exemplos | Versão |
|---|---|---|---|
| 1 — Afim | o mesmo pixel do fundo, linearmente | normal, multiply, screen, hard light, exclusion, inversão, tintas | 🔵 v1 |
| 2 — Curva | o mesmo pixel do fundo, não-linearmente | overlay, soft light, difference, darken/lighten, add com saturação, hue/sat/color/lum | 🟣 v2 |
| 3 — Vizinhança | pixels vizinhos do fundo | blur de fundo (vidro fosco), refração, distorção | 🟣 v2 |

## Contextos alvo (por prioridade)

1. **Web** (sites e apps no browser)
2. **Ferramentas de design** — Figma primeiro, depois Illustrator, Photoshop, Affinity
3. **Vídeo e motion** — After Effects, Premiere, DaVinci; mais tarde vídeo Vitral

## Requisitos funcionais

| ID | Requisito | Versão |
|---|---|---|
| RF1 | Um ficheiro Vitral define, para cada pixel e canal RGB, uma função do valor do fundo. Na v1 essa função é afim e é descrita por dois valores: **K** (resultado sobre preto) e **W** (resultado sobre branco). | 🔵 |
| RF2 | Modos exatos na v1: normal, multiply, screen, hard light, exclusion, inversão, e qualquer combinação destes por pixel, com opacidade e em pilha. Add, linear burn, darken/lighten e difference são aproximados pela "corda" (exatos sobre preto e branco). Overlay, soft light, dodge/burn e os modos HSL não são representáveis na v1 e as ferramentas devem avisar. | 🔵 |
| RF3 | O ficheiro é um **PNG válido**. Sem suporte Vitral, mostra uma imagem de recurso (*fallback*) razoável. | 🔵 |
| RF4 | Se o PNG for alterado por outra ferramenta (recompressão com perdas, edição), o descodificador **deteta-o** e usa apenas o fallback, em vez de mostrar lixo. | 🔵 |
| RF5 | **Autoria sem plugin**: qualquer ferramenta que exporte PNG pode criar um Vitral exportando a arte sobre fundo preto e sobre fundo branco. | 🔵 |
| RF6 | **Renderização sem código novo no anfitrião**: o Vitral pode ser mostrado empilhando 3 camadas com modos universais (difference, screen, multiply). | 🔵 |
| RF7 | **Web**: um polyfill progressivo. O HTML continua válido e mostra o fallback se o JavaScript falhar. | 🔵 |
| RF8 | **Figma**: plugin para exportar (frame → Vitral) e importar (Vitral → grupo de camadas com modos). | 🔵 |
| RF9 | **CLI**: codificar, descodificar, renderizar sobre um fundo, inspecionar, converter um PNG + modo. | 🔵 |
| RF10 | Camadas com modos não-lineares (nível 2). | 🟣 |
| RF11 | Efeitos de vizinhança, como o blur de fundo (nível 3). | 🟣 |
| RF12 | Animação e vídeo (ex.: light leaks Vitral). | 🟣 |
| RF13 | 16 bits por canal, HDR, wide gamut (Display P3), mistura em luz linear. | 🟣 |
| RF14 | Vetorial (perfil SVG). | 🟣 |

## Requisitos não funcionais

| ID | Requisito | Meta |
|---|---|---|
| RNF1 | Tamanho | Zonas em modo normal não custam bytes extra (residual nulo). Medido em out/2026: 2,5–4,7× o PNG nativo nas amostras (ver [05](05-verificacoes-v1.md)); a meta de +30% era irrealista para imagens que misturam em todo o lado. Proposta: ≤ 2,5× após melhorar a compressão. |
| RNF2 | Precisão | A pilha de 3 camadas difere da referência em ≤ 2 níveis (de 255). |
| RNF3 | Desempenho | Descodificar e preparar 1 megapixel em < 50 ms (WASM, portátil médio). |
| RNF4 | Segurança | Parser robusto (testes de *fuzzing*). Nenhum código acede aos pixels do fundo; só se usam os modos de mistura do anfitrião, com aritmética de tempo constante. |
| RNF5 | Abertura | Especificação livre de *royalties*; código MIT. |
| RNF6 | Cor | Mistura sobre valores sRGB codificados (não lineares), como CSS, Canvas, Figma e Photoshop por omissão. |
| RNF7 | Portabilidade | Núcleo em Rust, compilável para nativo e WebAssembly. |

## Fora de âmbito (v1)

* "Funcionar em qualquer lado sem suporte" — impossível (ver [estado da arte](01-estado-da-arte.md)).
* Imagens com perdas (JPEG/AVIF) como contentor.

## Questões

* **Q1 — Domínio**: em aberto.
  * Candidatos: `vitral.dev`, `vitral.app`, `vitral.design`, `vitralformat.org`,
    `getvitral.com`, `vitral.pt`.
  * Não foi possível confirmar a disponibilidade a partir deste ambiente, porque
    a rede bloqueia o RDAP e o DNS.
  * O repositório já foi renomeado para `vitral-image-format`.
* **Q2 — Vídeo**: adiado, com notas no [plano](04-plano.md).
* **Q3 — Contentor da v2**: só depois de fechar a v1 (fase 2.5 do plano).
* **Q4 — Fundo transparente**: ✅ decidido. Mostra a cor nativa (D9).
