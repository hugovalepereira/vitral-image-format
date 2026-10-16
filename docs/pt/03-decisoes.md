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
  * Chunk PNG: `viTr`.
* **Alternativas guardadas**: **Gel** (o filtro de cor da iluminação de palco, que
  é um multiply sobre a luz), Scrim, Mixel. Evitar **Glaze**, que é uma ferramenta
  anti-IA muito conhecida entre artistas.

## D2 — Contentor: **PNG + chunk auxiliar**

* **Contexto**: um formato novo começa com zero suporte; sem plugin aparece como
  "ficheiro partido".
* **Decisão**: o ficheiro é um PNG válido.
  * A imagem principal (`IDAT`) é o **fallback** que qualquer programa mostra.
  * Um chunk auxiliar `viTr` guarda o que falta para reconstruir K e W.
  * O chunk é *safe-to-copy* (quarta letra minúscula). Inicialmente era `vITR`
    (*unsafe*), mas os testes com 13 otimizadores mostraram que a marca *unsafe*
    só fazia as ferramentas descartá-lo. O CRC dos pixels já protege contra
    alterações (ver [05](05-verificacoes-v1.md)).
  * **Correção (out/2026):** o nome `vITr` marcava, por engano, um chunk
    *público*, porque nas regras do PNG é a segunda letra **minúscula** que
    indica um chunk privado. O nome passou a `viTr`. Se um dia o chunk for
    registado como público, passa a `vITr`.
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

## D7 — Espaço de cor: **valores codificados** (não lineares)

* **Decisão**: a mistura é feita sobre valores codificados (não lineares), como CSS
  Compositing nível 1, Canvas 2D, Figma e Photoshop por omissão. Assim o que o
  designer vê é o que a web mostra.
* Começou só em sRGB; desde D13 também em Display P3.
* **Para a v2**: uma flag de mistura em luz linear (útil em vídeo e jogos).

## D8 — Língua da documentação

* **Decisão**:
  * A especificação, o README e o código estão em **inglês**, para promoção
    internacional.
  * O planeamento e as decisões estão em **português**, em `docs/pt/`.

## D9 — Sem nada por trás: **mostra a cor nativa**

* **Contexto**: a pergunta Q4 era o que mostrar quando não há nada por trás.
  O autor respondeu: *"se não existir background, a cor deve ser simplesmente a
  cor nativa"*.
* **Decisão**: cada pixel tem uma **cor nativa** (RGBA), a mesma que uma camada
  mostra num programa de design sobre um canvas transparente.
  * Essa cor é a imagem principal do PNG, que é também o que os programas sem
    suporte mostram.
  * Sobre um fundo parcialmente transparente, o resultado mistura as duas
    coisas: `αfundo · f(fundo) + (1 − αfundo) · nativa`. É a mesma estrutura da
    fórmula W3C.
* **Porque tem de ser guardada**: não se deduz de K e W. Por exemplo, multiply
  com uma cor *s* e hard light com *s*/2 dão o mesmo K e W.
* **Autoria**: uma terceira exportação, o **PNG normal** que se exportaria hoje
  (com fundo transparente).
  * O formato aceita ficheiros sem ela, e nesse caso a cor nativa é aproximada.
  * Mas **as ferramentas exportam sempre as três** (decisão do autor, out/2026),
    porque o objetivo é o gráfico se comportar fora do Figma exatamente como
    dentro dele.
* **Quando é que "não há nada por trás"** (para evitar confusões):
  * **Quase nunca.** Qualquer coisa visível atrás da imagem é um fundo real: uma
    página, uma foto, uma cor, ou até um quadriculado *desenhado* por um
    programa. Nesses casos o Vitral reage ao fundo; é o caso normal.
  * Só não há fundo quando a imagem tem de ser **achatada antes de se saber onde
    vai ser colocada**. Por exemplo:
    * um frame sem preenchimento exportado para PNG;
    * um grupo isolado em CSS (com `opacity`, `transform` ou `isolation`), cujo
      conteúdo é composto à parte;
    * um programa sem suporte para Vitral.
  * Aí o resultado tem de ser um pixel RGBA normal, que não consegue guardar um
    modo de mistura, e a regra é mostrar a cor nativa, como o Figma faz.
  * Ilustração: `examples/renders/checker-backdrop-vs-nothing.png` (A: o
    quadriculado é o fundo; B: não há nada por trás).
* **Em termos de Figma** (*pass through* vs *normal*):
  * **Pass through** (padrão dos grupos e frames): as camadas lá dentro misturam-se
    diretamente com tudo o que está por baixo, incluindo o canvas.
  * **Normal**: o grupo é primeiro pintado numa folha transparente à parte (um
    "acetato") e só depois colado por cima. Os modos lá dentro só reagem ao
    conteúdo do próprio grupo.
  * Uma exportação PNG é sempre o acetato, ou seja, o frame em Normal. O Vitral
    exporta o comportamento *pass through*.
  * Equivalentes noutros programas:

    | Programa | Normal (isolado) | Pass through |
    |---|---|---|
    | Photoshop | grupo em Normal | grupo em Pass Through |
    | Illustrator | "Isolate Blending" ligado | "Isolate Blending" desligado |
    | CSS | `isolation: isolate`, `opacity` ou `transform` | sem isso |

  * Ilustração: `examples/renders/figma-canvas-vs-export.png`.
* **Consequência**: sem suporte, o Vitral mostra exatamente o PNG que se
  exportaria hoje. Nunca fica pior do que um PNG.
* **Limitação aceite**: a pilha de 3 camadas (CSS, Figma) só é exata sobre fundos
  opacos. Na web o fundo da página é sempre opaco; num programa de design, é
  preciso um fundo preenchido.

## D10 — Armazenamento: **diferenças ou K/W diretos, o que for menor**

* **Contexto**: em imagens que misturam muito, as diferenças em relação à cor
  nativa comprimem pior do que o próprio K/W, por causa do ruído de
  arredondamento.
* **Decisão**: o bit 0 das *flags* indica se o residual guarda as diferenças ou
  K e W diretamente. O codificador experimenta as duas formas e fica com a menor.

## D11 — Entradas: **só sRGB**

* **Contexto**: o Figma exporta no perfil do documento, que pode ser Display P3.
  Ler esses valores como sRGB estragava as cores sem aviso.
* **Decisão**: K, W e a cor nativa são sRGB.
  * O CLI deteta o espaço de cor de cada exportação (comparando as primárias do
    perfil ICC com as do sRGB) e recusa as que não são sRGB.
  * A demo deixa o browser converter.
* **Para depois**: conversão automática no CLI e suporte a Display P3 (v2).
* **Substituída por D13** (out/2026): o Display P3 passou a ser aceite.

## D12 — CRC da imagem principal: **ignora a cor dos pixels invisíveis**

* **Contexto**: o CRC da imagem principal incluía a cor dos pixels com alfa 0.
  Essa cor não se vê nem entra em K e W, mas otimizadores sem perdas
  (`oxipng -a`, `pngcrush -blacken`) limpam-na, e o ficheiro perdia o Vitral sem
  necessidade.
* **Decisão** (out/2026): o CRC conta cada pixel com alfa 0 como (0, 0, 0, 0)
  (SPEC §4.4).
  * A versão continua a 0.1, porque ainda não há ficheiros em circulação.
  * Um ficheiro feito antes desta mudança, com cor guardada em pixels
    transparentes, passa a ser lido como PNG normal. Os exemplos do repositório
    não têm esse caso e continuam válidos.

## D13 — Espaço de cor: **sRGB ou Display P3; luz linear fica de fora**

* **Contexto**:
  * No Figma, um ficheiro em Display P3 guarda valores P3 e mistura-os nesse
    espaço (deduzido; não confirmado com o Figma).
  * Os browsers misturam os blend modes do CSS num espaço que escolhem. O Chromium
    testado mistura em sRGB, mesmo com um ecrã P3 simulado; há browsers no Mac
    que misturam em P3 quando o ecrã o permite ([discussão no W3C][fxtf]).
  * A mesma mistura feita em P3 ou em sRGB difere em média 3 a 7 níveis (em 255)
    e muito mais em cores saturadas: não é um detalhe.
* **Decisão**:
  * O ficheiro guarda o espaço de cor da arte, declarado como em qualquer PNG
    (perfil ICC e `cICP` para P3). K, W e a cor nativa ficam nesse espaço, e a
    mistura é feita lá (SPEC §2.4).
  * Exportações em espaços diferentes são convertidas para Display P3, que
    contém o sRGB.
  * Quem mistura noutro espaço converte K e W para o seu antes de montar as 3
    camadas. O polyfill assume sRGB: é exato sobre preto e branco e aproximado
    no resto, como o próprio `mix-blend-mode` entre ecrãs diferentes.
  * **Luz linear fica de fora.** Nem o Figma nem o CSS misturam em luz linear;
    só algumas ferramentas de vídeo e 3D. Ficheiros marcados como lineares
    (`gAMA` 1,0, curva ICC de gama 1, `cICP` com transferência 8) são recusados
    com uma mensagem clara. Volta a ser estudada com o After Effects (v2).
* **Consequência**: um Vitral feito num ficheiro P3 é exato no Figma em P3 (com o
  plugin), no compositor de referência e nos browsers que misturam em P3.

[fxtf]: https://lists.w3.org/Archives/Public/public-fxtf-archive/2023Jul/0007.html
