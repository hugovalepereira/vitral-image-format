# 01 — Estado da arte

> Pergunta de partida: *existe um formato de imagem que traga consigo o modo de
> mistura (blend mode), de forma a reagir automaticamente ao que está por trás
> dele, tal como um PNG traz a transparência?*

**Resposta curta: não, como formato de entrega.** Existem peças soltas — formatos
de edição com camadas, formatos de VFX com alfa pré-multiplicado, um formato
moderno (JPEG XL) com modos de mistura *internos* — mas nenhum formato leva a
intenção de mistura até ao **anfitrião** (a página web, o documento de design, a
timeline de vídeo) onde a imagem é colocada.

## Porque é que não existe: o anfitrião é dono da composição

Um descodificador de imagem devolve pixels. Quem decide *como* esses pixels se
juntam ao fundo é o anfitrião (browser, Figma, After Effects…), e por omissão usa
sempre o mesmo operador: *source-over* com alfa. A especificação W3C de
Compositing & Blending é explícita: o conteúdo de um `<img>` ou de uma
`background-image` é sempre desenhado num **grupo isolado** — um SVG carregado via
`<img>` não se mistura com o fundo da página, mesmo que tenha `mix-blend-mode` lá
dentro ([W3C Compositing-1][compositing]).

Consequências que moldam todo o projeto:

1. **"Em qualquer situação" é impossível por construção.** Onde não houver suporte
   (visualizador de fotos do SO, WhatsApp, email), a imagem só pode mostrar uma
   aproximação. O objetivo realista é *funcionar onde há suporte e degradar com
   elegância no resto*.
2. **Mesmo com suporte, a imagem só reage ao que está no mesmo contexto de
   empilhamento** (em CSS, qualquer `opacity`, `transform`, `filter` ou `z-index`
   num antepassado cria um grupo isolado) — a mesma limitação que o
   `mix-blend-mode` já tem ([W3C Compositing-1][compositing]).
3. **Efeitos dependentes do fundo são sensíveis em segurança.** Em 2018 foi
   demonstrado um ataque de *timing* com `mix-blend-mode` que lia pixels de iframes
   de outra origem (Facebook); foi corrigido no Chrome 63 e Firefox 60
   ([The Daily Swig][timing]). Qualquer proposta para browsers tem de mostrar que
   não abre canais destes (aritmética de tempo constante, nenhum acesso do código
   aos pixels do fundo).

## O que existe hoje

| Tecnologia | O que faz | Porque não resolve |
|---|---|---|
| **CSS `mix-blend-mode`**, Canvas `globalCompositeOperation` | Aplica 1 modo a um elemento colocado. Suporta os 16 modos W3C + `plus-lighter` (Chrome, Safari, Firefox ≥ 99) ([MDN][mdn-mbm], [caniuse][caniuse-pl]) | O modo vive no **CSS**, não no ficheiro. Um modo por elemento. Perde-se na passagem designer → dev. |
| **Figma, Photoshop, Illustrator, AE, Affinity** | Modos de mistura por camada | Ao exportar PNG, tudo é achatado contra transparente. A intenção perde-se. |
| **SVG com `mix-blend-mode`** | Mistura funciona *inline* no HTML | Via `<img>`/`background-image` é isolado ([W3C][compositing]). |
| **PSD, TIFF com camadas, XCF, KRA, OpenRaster** | Camadas com modos. O OpenRaster usa `composite-op="svg:multiply"` etc. ([OpenRaster][ora]) | Formatos de **edição**; achatados antes de chegar ao anfitrião. |
| **JPEG XL** | *Frames*/camadas com modos de mistura: Replace, Add, Blend, AlphaWeightedAdd, Mul ([paper JPEG XL][jxl-paper], [libjxl][jxl-hdr]) | A mistura é **interna** (camadas entre si); o resultado final entra no anfitrião com *source-over*. O Krita só exporta Normal e Addition ([Krita][krita-jxl]). Relevante: o Chrome 155 (out/2026) passou a suportar JXL ([Chrome][chrome-jxl]). |
| **PDF / AI colocados no InDesign** | Grupos de transparência *não isolados*: um objeto em Multiply dentro do PDF pode afetar o que está por baixo na página | O único fluxo *mainstream* onde "o ficheiro traz o modo" funciona — mas só em impressão/paginação, e com comportamento inconsistente reportado ([fórum Adobe][indesign]). |
| **OpenEXR / alfa pré-multiplicado** | Pixel com alfa 0 e cor > 0 é válido e representa luz emitida (ex.: chama de vela) — mistura *normal* e *aditiva* por pixel | Só VFX; o PNG usa alfa direto e não consegue representar isto ([OpenEXR docs][exr]). |
| **APNG** (PNG 3.ª edição) | `blend_op` entre *frames* (SOURCE / OVER) ([PNG 3][png3]) | Só entre frames da animação. |
| **Convenções de assets** (light leaks, overlays, partículas) | Distribuídos "sobre preto, usar em modo Screen" ou "sobre branco, usar em Multiply" | É uma *instrução ao utilizador*, não um dado do ficheiro. Exatamente o problema que o Vitral resolve. |
| **GPU** | *Dual-source blending* (WebGPU desde Chrome 130, [Chrome][webgpu-dsb]): `resultado = src0 + src1 × fundo` numa só passagem | É a primitiva ideal para renderizar o Vitral nativamente — mas não é um formato. |

## A descoberta que estrutura o Vitral

Para os modos mais usados, o resultado de cada canal (R, G, B) de cada pixel é uma
**função afim do fundo**:

```
resultado = K + (W − K) × fundo        (valores em [0, 1])
```

* **K** = como o pixel fica sobre **preto**
* **W** = como o pixel fica sobre **branco**

| Modo (com opacidade α) | Exato? |
|---|---|
| Normal | ✅ (é o caso particular do PNG: W − K igual nos 3 canais) |
| Multiply | ✅ |
| Screen | ✅ |
| Hard light | ✅ |
| Exclusion | ✅ |
| Inversão (difference com branco) | ✅ |
| Tintas/vidro colorido (qualquer combinação dos anteriores, por pixel, em pilha) | ✅ |
| Add / plus-lighter, linear burn, darken/lighten, difference genérico | ≈ exato sobre preto e branco, linear entre os dois (add comporta-se como screen; difference como exclusion) |
| Overlay, soft light, color dodge/burn, hue/saturation/color/luminosity | ❌ **perdidos**: estes modos deixam o preto e o branco intactos, por isso a captura K/W não os "vê" (nível 2, v2) |
| Blur de fundo, refração (dependem dos pixels vizinhos) | ❌ (nível 3, v2) |

Propriedades que tornam este modelo uma base forte para a v1:

1. **Generaliza o PNG**: "o PNG tem cor + opacidade; o Vitral tem luz + filtro".
2. **Modo por pixel**: sombra em multiply, corpo em normal, brilho em screen —
   tudo numa só imagem, sem camadas.
3. **Pilhas de camadas afins colapsam num único par K/W.**
4. **Autoria sem plugin**: exportar a arte duas vezes, sobre preto e sobre branco.
5. **Renderização com os 3 modos mais universais** (difference → screen →
   multiply), disponíveis em CSS, Canvas, SVG, Figma, Illustrator, Photoshop,
   After Effects, Affinity…

[compositing]: https://www.w3.org/TR/compositing-1/
[timing]: https://portswigger.net/daily-swig/a-dangerous-blend-side-channel-attack-exploits-css-vulnerability
[mdn-mbm]: https://developer.mozilla.org/en-US/docs/Web/CSS/mix-blend-mode
[caniuse-pl]: https://caniuse.com/mdn-css_properties_mix-blend-mode_plus-lighter
[ora]: https://www.openraster.org/baseline/layer-stack-spec.html
[jxl-paper]: https://arxiv.org/pdf/2506.05987
[jxl-hdr]: https://github.com/libjxl/libjxl/blob/f88745497118727f861cb00887cadcb395d10f1c/doc/format_overview.md
[krita-jxl]: https://docs.krita.org/en/general_concepts/file_formats/file_jxl.html
[chrome-jxl]: https://developer.chrome.com/blog/jpeg-xl-in-chrome
[indesign]: https://community.adobe.com/t5/indesign-discussions/blend-mode-in-ai-graphic-will-not-work-in-indesign-when-placed/m-p/10106103
[exr]: https://cary-ilm-openexr.readthedocs.io/en/latest/_sources/TechnicalIntroduction.rst.txt
[png3]: https://www.w3.org/TR/png-3/
[webgpu-dsb]: https://developer.chrome.com/blog/new-in-webgpu-130
