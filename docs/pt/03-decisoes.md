# 03 — Registo de decisões

Cada decisão tem contexto, a opção escolhida e as alternativas **guardadas**, que
podem voltar a ser consideradas.

## D1 — Nome: **Vitral**

* **Contexto**: "GFI" é um anagrama de GIF e confunde-se ao falar e ao escrever.
  `.gfi` já é usado (dados financeiros GIFI/QuickBooks, Golden Software, entre
  outros) e "GFI Software" é uma empresa conhecida.
* **Decisão**: **Vitral**. Um vitral filtra a luz que vem de trás (transmissão) e
  brilha com cor própria (emissão), que é exatamente o modelo K/W. Tem origem
  portuguesa e uma boa história.
* **Convenções**:
  * Ficheiros com dupla extensão **`nome.vitral.png`**, à semelhança dos `.9.png`
    do Android.
  * Tipo MIME: `image/png`.
  * Chunk PNG: `vITr`.
* **Alternativas guardadas**: **Gel** (o filtro de cor da iluminação de palco, que
  é um multiply sobre a luz), Scrim, Mixel. Evitar **Glaze**, que é uma ferramenta
  anti-IA muito conhecida entre artistas.

## D2 — Contentor: **PNG + chunk auxiliar**

* **Contexto**: um formato novo começa com zero suporte; sem plugin aparece como
  "ficheiro partido".
* **Decisão**: o ficheiro é um PNG válido.
  * A imagem principal (`IDAT`) é o **fallback** que qualquer programa mostra.
  * Um chunk auxiliar `vITr` guarda o que falta para reconstruir K e W.
  * Um CRC da imagem principal deteta recompressões ou edições.
* **Riscos aceites**: CDNs e otimizadores que retiram chunks desconhecidos fazem a
  imagem cair para o fallback. Isto é degradação, não erro.
* **Alternativas guardadas**:
  * **Contentor ZIP próprio** (manifest JSON + PNGs + preview, como
    OpenRaster, `.kra` ou `.docx`). É mais fácil de estender a camadas e efeitos
    (v2).
  * **Ambos**: PNG para distribuir e ZIP para autoria. Hipótese forte para a v2.
  * **Extensão de JPEG XL**, que já tem camadas com Mul/Add internamente e passou a
    ter suporte no Chrome 155. A explorar como segundo mapeamento.

## D3 — Modelo da v1: **afim por pixel, guardado como K/W**

* **Contexto**:
  * O autor quer o formato "o mais completo possível", mas começar
    pragmaticamente.
  * As motivações principais são multiply e semelhantes, luz/brilhos e inversão.
  * O blur e os modos complexos ficam para a fase 2.
* **Decisão**: cada canal de cada pixel guarda K (sobre preto) e W (sobre branco),
  e o resultado é `K + (W − K) × fundo`.
  * Cobre todas as funções afins de [0,1] em [0,1], incluindo as decrescentes
    (inversão).
  * Não precisa de valores com sinal.
  * Nunca precisa de *clamping*.
* **Porque não E/T** (emissão + transmissão com sinal e *clamp*):
  * Representaria `add` exatamente.
  * Mas exige valores com sinal e maiores que 1, e uma pilha de renderização mais
    rara (`plus-lighter`, que não existe no Illustrator).
  * O K/W é canónico, autorável sem plugin e renderizável em todo o lado.
* **Alternativas guardadas**:
  * Camadas com modos nomeados (mini-PSD), que entram na v2 como extensão.
  * Modelo E/T, possível como perfil de precisão no futuro.

## D4 — Renderização: **difference → screen → multiply**

* **Decisão**: para cada canal:
  * `N = 1 se W < K, senão 0`
  * `S = min(K,W) / max(K,W)`
  * `M = max(K,W)`
* **Pilha** (de baixo para cima, sobre o fundo):
  1. difference com N
  2. screen com S
  3. multiply com M
* **Porquê**: são três dos modos mais universais (CSS, Canvas, SVG, Figma,
  Illustrator, Photoshop, After Effects, Affinity). Quando nada é invertido, a
  camada de difference é vazia e pode ser omitida.
* **Alternativa guardada**: multiply com |W − K| seguido de plus-lighter com
  min(K,W). É mais precisa onde existe `plus-lighter`.

## D5 — Stack técnica: **Rust**

* **Decisão**:
  * O núcleo, o CLI e a implementação de referência são em Rust.
  * As bindings WebAssembly (`wasm-bindgen`) servem o polyfill web e o plugin
    Figma.
  * Os plugins continuam a ter uma camada fina de JS/TS, porque é a única
    linguagem que o Figma e a Adobe aceitam.
* **Alternativa guardada**: um descodificador mínimo em JS puro (via
  `DecompressionStream`) se o tamanho do WASM se tornar um problema no polyfill.

## D6 — Primeiro plugin: **Figma**

* **Decisão**: o Figma vem primeiro.
  * Plugins em TS/web com suporte a WASM.
  * Tem difference, screen, multiply e *background blur* (útil para o nível 3).
  * Tem uma comunidade grande para divulgação.
* **Seguintes**: Illustrator (script, sem blur de fundo possível), After Effects,
  Photoshop.

## D7 — Espaço de cor: **sRGB codificado**

* **Decisão**: a mistura é feita sobre valores codificados (não lineares), como CSS
  Compositing nível 1, Canvas 2D, Figma e Photoshop por omissão. Assim o que o
  designer vê é o que a web mostra.
* **Para a v2**: uma flag de mistura em luz linear (útil em vídeo e jogos).

## D8 — Língua da documentação

* **Decisão**:
  * A especificação, o README e o código estão em **inglês**, para promoção
    internacional.
  * O planeamento e as decisões estão em **português**, em `docs/pt/`.
