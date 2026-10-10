// SPDX-License-Identifier: GPL-3.0-or-later
// Plantilla del presupuesto en PDF (Typst). Se puede copiar y retocar:
// tipografía, márgenes, colores, portada, pie... Los datos llegan en
// «datos.json» (importes ya formateados en notación española).

#let d = json("datos.json")
#let acento = rgb("#1f4e79")
#let gris = luma(110)

#set document(title: d.titulo, author: d.autor)
#set text(font: "New Computer Modern", size: 9.5pt, lang: "es", region: "es", hyphenate: true)
#set par(justify: true, leading: 0.55em, spacing: 0.8em)

// ------------------------------------------------------------------ portada
#page(margin: (x: 2.5cm, top: 2.5cm, bottom: 2.5cm), header: none, footer: none)[
  #if d.logo != none [
    #align(right, image(d.logo, height: 2.2cm))
  ]
  #v(1fr)
  #text(size: 11pt, fill: gris, tracking: 0.15em)[#upper(d.tipo_documento)]
  #v(0.4em)
  #line(length: 100%, stroke: 1.2pt + acento)
  #v(0.6em)
  #par(justify: false, text(size: 22pt, weight: "bold", fill: acento, d.titulo))
  #if d.direccion != "" [
    #v(0.3em)
    #text(size: 12pt, d.direccion)
  ]
  #v(2.5em)
  #grid(
    columns: (3.6cm, 1fr),
    row-gutter: 0.75em,
    ..if d.cliente != "" { (text(fill: gris)[Cliente], text(weight: "bold", d.cliente)) },
    text(fill: gris)[Referencia], d.referencia,
    text(fill: gris)[Revisión], text(weight: "bold", d.revision),
    text(fill: gris)[Fecha], d.fecha,
    text(fill: gris)[Importe (IVA incl.)], text(weight: "bold")[#d.total €],
  )
  #v(1fr)
  #line(length: 100%, stroke: 0.4pt + gris)
  #grid(
    columns: (1fr, auto),
    align: (left + top, right + top),
    [#text(weight: "bold", d.empresa) \ #text(fill: gris, d.empresa_datos)],
    [#d.autor \ #text(fill: gris, d.autor_datos)],
  )
]

// ------------------------------------------------- páginas del presupuesto
#counter(page).update(1)
#set page(
  paper: "a4",
  margin: (x: 2cm, top: 2.6cm, bottom: 2.2cm),
  header: context [
    #set text(size: 8pt, fill: gris)
    #grid(
      columns: (1fr, auto),
      [#d.titulo], [#d.referencia · Rev. #d.revision],
    )
    #v(-0.5em)
    #line(length: 100%, stroke: 0.4pt + gris)
  ],
  footer: context [
    #set text(size: 8pt, fill: gris)
    #grid(
      columns: (1fr, auto),
      (d.empresa, d.fecha).filter(x => x != "").join(" · "),
      [Página #counter(page).get().first() de #counter(page).final().first()],
    )
  ],
)

#let num(s) = align(right, s)

// Una partida: código, unidad, resumen, texto, medición y cantidad × precio = importe
#let partida(p) = block(breakable: true, width: 100%, inset: (y: 0.35em))[
  #grid(
    columns: (2.3cm, 1.1cm, 1fr),
    column-gutter: 0.6em,
    text(weight: "bold", p.codigo), p.unidad, text(weight: "bold", p.resumen),
  )
  #if d.mostrar_textos and p.texto != "" [
    #pad(left: 4.0cm, text(size: 9pt, p.texto))
  ]
  #if d.mostrar_mediciones and p.mediciones.len() > 0 [
    #pad(left: 4.0cm, table(
      columns: (1fr, 1.3cm, 1.5cm, 1.5cm, 1.5cm, 1.8cm),
      stroke: none,
      inset: (x: 3pt, y: 1.6pt),
      align: (left, right, right, right, right, right),
      table.header(
        ..([Comentario], [Uds.], [Longitud], [Anchura], [Altura], [Parcial]).map(t => text(size: 7.5pt, fill: gris, t)),
      ),
      table.hline(stroke: 0.3pt + gris),
      ..p.mediciones.map(m => (
        text(size: 8.5pt, style: if m.subtotal { "italic" } else { "normal" }, m.comentario),
        text(size: 8.5pt, m.uds), text(size: 8.5pt, m.largo), text(size: 8.5pt, m.ancho),
        text(size: 8.5pt, m.alto),
        text(size: 8.5pt, weight: if m.subtotal { "bold" } else { "regular" }, m.parcial),
      )).flatten(),
    ))
  ]
  #v(0.15em)
  #align(right, grid(
    columns: (auto, 2.2cm, auto, 2.0cm, auto, 2.4cm),
    column-gutter: 0.5em,
    align: (right, right, right, right, right, right),
    text(fill: gris, size: 8pt)[Cantidad], p.cantidad,
    text(fill: gris, size: 8pt)[Precio], p.precio,
    text(fill: gris, size: 8pt)[Importe], text(weight: "bold", p.importe),
  ))
  #line(length: 100%, stroke: 0.25pt + luma(200))
]

// Capítulo (recursivo: puede contener subcapítulos)
#let capitulo(c) = {
  let tam = if c.nivel == 1 { 12pt } else { 10.5pt }
  block(sticky: true, above: 1.2em, below: 0.6em, width: 100%, {
    set par(justify: false)
    text(size: tam, weight: "bold", fill: acento)[#c.codigo #h(0.8em) #upper(c.resumen)]
    v(-0.4em)
    line(length: 100%, stroke: (if c.nivel == 1 { 0.8pt } else { 0.4pt }) + acento)
  })
  for e in c.elementos {
    if e.tipo == "capitulo" { capitulo(e) } else { partida(e) }
  }
  block(above: 0.4em, below: 1.0em, width: 100%, align(right, text(weight: "bold")[
    Total #c.codigo #h(1em) #box(width: 2.6cm, align(right, c.importe)) €
  ]))
}

#for c in d.capitulos { capitulo(c) }

// ------------------------------------------------------------- resumen
#pagebreak(weak: true)
#text(size: 14pt, weight: "bold", fill: acento)[RESUMEN DEL PRESUPUESTO]
#v(-0.4em)
#line(length: 100%, stroke: 0.8pt + acento)
#v(0.5em)
#table(
  columns: (2.2cm, 1fr, 3cm, 1.6cm),
  stroke: none,
  inset: (x: 4pt, y: 3pt),
  align: (left, left, right, right),
  table.header(..([Código], [Capítulo], [Importe €], [%]).map(t => text(fill: gris, size: 8.5pt, t))),
  table.hline(stroke: 0.4pt + gris),
  ..d.resumen.map(r => (
    pad(left: (r.nivel - 1) * 0.8em, if r.nivel == 1 { strong(r.codigo) } else { r.codigo }),
    pad(left: (r.nivel - 1) * 0.8em, if r.nivel == 1 { strong(r.resumen) } else { r.resumen }),
    if r.nivel == 1 { strong(r.importe) } else { r.importe },
    text(fill: gris, r.pct),
  )).flatten(),
)
#v(1em)
#align(right, block(width: 10.5cm, table(
  columns: (1fr, 3.2cm),
  stroke: none,
  inset: (x: 4pt, y: 3.5pt),
  align: (left, right),
  [*Presupuesto de ejecución material*], [*#d.pem €*],
  ..d.recargos.map(r => (r.concepto, r.importe)).flatten(),
  table.hline(stroke: 0.4pt + gris),
  [*#d.etiqueta_base*], [*#d.base €*],
  ..if d.iva != none { ([#d.iva.concepto], [#d.iva.importe]) },
  table.hline(stroke: 1pt + acento),
  text(weight: "bold", fill: acento)[TOTAL PRESUPUESTO], text(weight: "bold", fill: acento)[#d.total €],
)))
#v(1.2em)
#par[Asciende el presente presupuesto a la expresada cantidad de *#upper(d.total_letra)* (#d.total €)#if d.iva != none [, IVA incluido].]

#if d.autor != "" or d.lugar_fecha != "" [
  #v(2.5em)
  #align(right, block(width: 8cm, align(center)[
    #d.lugar_fecha
    #v(3em)
    #line(length: 70%, stroke: 0.4pt)
    #d.autor \
    #text(fill: gris, size: 8.5pt, d.autor_datos)
  ]))
]

// ------------------------------------------------------- observaciones
#if d.observaciones.len() > 0 [
  #pagebreak()
  #text(size: 14pt, weight: "bold", fill: acento)[OBSERVACIONES AL PRESUPUESTO]
  #v(-0.4em)
  #line(length: 100%, stroke: 0.8pt + acento)
  #v(0.6em)
  #set enum(numbering: "1.", spacing: 0.9em)
  #for o in d.observaciones [+ #o]
]
