# 05 — Verificações da v1 (outubro de 2026)

Objetivo: obter informação real sobre a v1 antes de decidir a v2. Cada secção
diz o que foi testado, o resultado, o que mudou por causa disso e como
reproduzir o teste.

## 1. Sobrevivência a otimizadores e conversores de PNG

**Pergunta:** quando um site, um CDN ou um designer passa um `.vitral.png` por
uma ferramenta de otimização, os dados Vitral sobrevivem?

**Como reproduzir:** `tools/optimizer-survival.sh`

| Ferramenta | Resultado |
|---|---|
| optipng (por omissão) | ✅ mantém (ficheiro fica 3–22% menor) |
| oxipng (por omissão) | ✅ mantém (9–24% menor) |
| pngcrush (por omissão e `-rem allb`) | ✅ mantém, depois da mudança para `vITr` (antes removia) |
| zopflipng com `--keepchunks=vITr` | ✅ mantém |
| zopflipng (por omissão), advpng, `optipng -strip all`, `oxipng --strip safe/all` | ⚪ remove o chunk |
| pngquant (com perdas), ImageMagick, Pillow, ffmpeg, sharp/libvips | ⚪ remove o chunk |

**Conclusões:**

- **Nenhuma ferramenta produziu um resultado errado.** Quando o chunk desaparece,
  o ficheiro passa a ser o PNG com as cores nativas, que é exatamente o PNG que se
  exportaria hoje.
- **O sharp/libvips remove o chunk.** É a biblioteca do Next.js e de muitos CDNs
  (Cloudinary, imgix e semelhantes recodificam as imagens). Em produção, os
  ficheiros Vitral têm de ser servidos sem otimização automática; no Next.js,
  por exemplo, com `unoptimized`. Isto tem de constar na documentação de uso.
- **Mudança feita:** o chunk passou de `vITR` (*unsafe-to-copy*) para `vITr`
  (*safe-to-copy*). A marca *unsafe* não protegia nada, porque o descodificador
  já verifica os pixels com um CRC, e só fazia as ferramentas descartarem o
  chunk.
- **Pista para depois:** os otimizadores *sem perdas* reduzem os nossos ficheiros
  9–24%. O codificador pode comprimir melhor (por exemplo, usando a biblioteca
  do oxipng no CLI).

## 2. Tamanho dos ficheiros

Medido nos exemplos: a parte Vitral (o residual) ocupava **63–78% do ficheiro**.
A meta RNF1 (< +30% sobre o PNG equivalente) **não é cumprida** em imagens com
muita mistura.

| Exemplo | Ficheiro | Só o PNG nativo (oxipng) | Antes da otimização |
|---|---|---|---|
| vitral | 88,9 KB | 28,0 KB | 98,2 KB |
| vidro âmbar | 38,6 KB | 13,9 KB | 44,9 KB |
| light leak | 130,1 KB | 51,2 KB | 172,5 KB |
| anel que inverte | 20,2 KB | 4,3 KB | 20,3 KB |

**Mudanças feitas:**

- **Cor nativa como imagem principal**, em vez da aproximação: ficheiros 0–14%
  menores.
- **K e W guardados diretamente** quando isso dá um ficheiro menor (bit 0 das
  *flags*): light leak −18%.

**Diagnóstico:** um pixel que mistura precisa de 6 valores (K e W em RGB) em vez
dos 4 de um PNG. Por isso, em imagens que misturam em todo o lado, cerca de 2 a
2,5× o tamanho do PNG é um mínimo realista.

**Próximos passos possíveis:**

- melhor compressão (deflate ao nível do oxipng);
- prever K e W a partir de um modo provável (multiply, screen);
- 4:2:0 nos canais de transmissão.

A meta RNF1 deve ser revista para algo como "≤ 2,5× o PNG nativo".

## 3. Robustez do descodificador (*fuzzing*)

**Como reproduzir:** `VITRAL_FUZZ_ITERATIONS=300000 cargo test --release --test robustness`

**Teste:**

- 300 000 mutações aleatórias do conteúdo do chunk, com o CRC do chunk
  recalculado para que os danos cheguem ao interior do descodificador;
- mais 300 000 mutações em qualquer parte do ficheiro.

**Resultado:** **zero *panics***. Os erros acabam sempre num estado claro
(*Corrupt*, *Unsupported*, *FallbackMismatch*) e o PNG continua visível.

Também confirmado: uma imagem principal regravada em paleta, em cinzento com
alfa ou entrelaçada (Adam7) continua válida. O CRC é calculado sobre os pixels
normalizados, não sobre os bytes do ficheiro.

## 4. Browsers

**Como reproduzir:** `VITRAL_BROWSERS=chromium,firefox,webkit node web/test/run.mjs`.
Corre automaticamente no CI.

**Resultado:** os quatro exemplos coincidem com o compositor de referência em
Rust nos três motores (Chromium, Firefox e WebKit, que é o motor do Safari). O
erro máximo é de 2 níveis em 255 e o erro médio fica entre 0 e 0,05. Os três
motores dão **exatamente os mesmos números**.

## 5. Gestão de cor

**Problema encontrado:** o Figma exporta, por omissão, no perfil de cor do
documento. Um ficheiro em Display P3 exporta PNGs em P3. Até aqui, o codificador
lia esses valores como sRGB, e as cores e a mistura ficariam erradas sem aviso.

**Mudanças feitas:**

- **Especificação (§2.4):** K, W e a cor nativa são valores **sRGB**. Os
  codificadores têm de converter ou recusar outros espaços de cor.
- **Biblioteca:** `colour_space()` identifica o espaço declarado pelo PNG
  (`cICP`, perfil ICC, `sRGB`, `cHRM`). Num perfil ICC, compara as **cores
  primárias** com as do sRGB em vez de confiar no nome. Testado com perfis reais
  do LittleCMS: um perfil com primárias P3 e o nome "sRGB built-in" foi
  corretamente recusado.
- **CLI:** `vitral encode` recusa exportações noutro espaço de cor, com uma
  mensagem a explicar como exportar em sRGB. `--assume-srgb` força o uso.
- **Demo web:** o browser converte automaticamente as exportações com perfil para
  sRGB.

**Para depois:**

- conversão automática no CLI (biblioteca `moxcms`, em Rust puro);
- suporte nativo a Display P3 (v2).

## 6. Exportações reais do Figma

O autor exportou do Figma duas artes, cada uma sobre preto, sobre branco e sobre
transparente (184 × 184 px):

| Arte | Composição | Ficheiro Vitral | PNG simples |
|---|---|---|---|
| **multiply** | Retângulo laranja e círculo verde, ambos em multiply. | 5,1 KB | 2,5 KB |
| **mixed** | Retângulo laranja em **screen**, círculo verde em **multiply** e triângulo ciano em **normal a 50%**, sobrepostos. | 7,8 KB | — |

**Resultados:**

- **As exportações vêm marcadas como sRGB** (chunks `sRGB` e `gAMA`), por isso
  passam a verificação de cor (D11).
- **Nas duas artes, o ficheiro Vitral reproduz as três exportações sem um único
  valor de diferença.**
- **Sobre outros cinzentos** (40, 128 e 210), cada zona segue a fórmula do seu
  modo, com um erro de no máximo 0,5 antes do arredondamento:

  | Zona | Fórmula |
  |---|---|
  | Retângulo | screen |
  | Círculo | multiply |
  | Triângulo | normal a 50% |
  | Círculo sobre o retângulo | multiply sobre screen |

- **O comportamento do Figma sobre transparente coincide com a D9:** cada forma
  mostra a sua cor e só mistura onde há alguma coisa por baixo. Por exemplo, o
  círculo em multiply sobre o retângulo em screen dá a cor do produto. Isto
  confirma a regra da cor nativa com uma ferramenta real.
- **Porque a cor nativa tem de ser guardada:** o screen faz o retângulo desaparecer sobre branco,
  mas sobre transparente aparece com a sua cor. A cor nativa é guardada à parte
  precisamente por isso.

Os ficheiros ficaram como teste permanente:

- dados em `crates/vitral/tests/data/figma/`;
- teste em `crates/vitral/tests/real_exports.rs`;
- comparações visuais em `examples/renders/figma-multiply.png` e
  `examples/renders/figma-mixed.png`.

## 7. O que continua por verificar

- **Exportações reais de outras ferramentas** (Photoshop, Illustrator, After
  Effects).
- **Testes de conformidade publicados:** ficheiros de referência e resultados
  esperados, para outras implementações.
- **Desempenho em imagens grandes** (meta RNF3: 1 megapixel em menos de 50 ms no
  browser).
