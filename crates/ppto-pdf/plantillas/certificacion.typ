// SPDX-License-Identifier: GPL-3.0-or-later
// Certificación de obra: relación valorada y resumen (Typst). Los datos
// llegan en «datos.json» con los importes ya formateados (1.234,56).

#let d = json("datos.json")
#let acento = rgb("#1f4e79")
#let gris = luma(110)
#let rojo = rgb("#c0392b")

#set document(title: d.titulo, author: d.empresa)
#set text(font: "New Computer Modern", size: 8pt, lang: "es", region: "es")
#set page(
  paper: "a4",
  flipped: true,
  margin: (x: 1.3cm, top: 1.4cm, bottom: 1.6cm),
  footer: context [
    #set text(size: 7pt, fill: gris)
    #grid(columns: (1fr, auto), [#d.titulo · #d.obra · #d.fecha],
      [Página #counter(page).display() de #counter(page).final().first()])
  ],
)

#grid(
  columns: (1fr, auto),
  align: (left + top, right + top),
  [
    #text(size: 9pt, fill: gris, tracking: 0.12em)[RELACIÓN VALORADA]
    #v(0.1em)
    #text(size: 17pt, weight: "bold", fill: acento, d.titulo)
    #v(0.1em)
    #text(size: 10pt, weight: "bold", d.obra)
  ],
  [
    #if d.logo != none [#image(d.logo, height: 1.4cm) \ ]
    #text(weight: "bold", d.empresa) \
    #text(size: 7.5pt, fill: gris, d.empresa_datos)
  ],
)
#v(0.2em)
#line(length: 100%, stroke: 0.8pt + acento)
#grid(columns: (auto, 1fr, auto, 1fr), column-gutter: 1em, row-gutter: 0.4em,
  text(fill: gris)[Fecha], d.fecha,
  ..if d.cliente != "" { (text(fill: gris)[Cliente], d.cliente) } else { ([], []) },
  ..if d.referencia != "" { (text(fill: gris)[Referencia], d.referencia, [], []) },
)
#v(0.5em)

#let cab(x) = text(weight: "bold", fill: white, size: 7.5pt, x)
#table(
  columns: (auto, 1fr, auto, auto, auto, auto, auto, auto, auto, auto, auto, auto),
  stroke: (x, y) => if y <= 1 { none } else { (bottom: 0.3pt + luma(210)) },
  fill: (x, y) => if y <= 1 { acento } else { none },
  align: (x, y) => if y > 1 and x >= 3 { right } else if y <= 1 and x >= 3 { center } else { left },
  inset: (x: 3pt, y: 3pt),
  table.header(
    table.cell(rowspan: 2, cab[Código]), table.cell(rowspan: 2, cab[Descripción]),
    table.cell(rowspan: 2, cab[Ud]), table.cell(rowspan: 2, cab[Presup.]), table.cell(rowspan: 2, cab[Precio €]),
    table.cell(colspan: 3, cab[Cantidad]), table.cell(colspan: 3, cab[Importe €]), table.cell(rowspan: 2, cab[%]),
    cab[Anterior], cab[Mes], cab[Origen], cab[Anterior], cab[Mes], cab[Origen],
  ),
  ..for f in d.filas {
    if f.capitulo {
      let s(x) = text(weight: "bold", fill: if f.nivel == 0 { acento } else { black }, x)
      (
        table.cell(fill: luma(240), s(f.codigo)),
        table.cell(fill: luma(240), colspan: 7, s(f.resumen)),
        table.cell(fill: luma(240), s(f.anterior)),
        table.cell(fill: luma(240), s(f.actual)),
        table.cell(fill: luma(240), s(f.origen)),
        table.cell(fill: luma(240), text(fill: gris, f.pct)),
      )
    } else {
      let x(v) = if f.exceso { text(fill: rojo, weight: "bold", v) } else { v }
      (
        f.codigo, f.resumen, f.unidad, f.medicion, f.precio,
        text(fill: gris, f.c_anterior), f.c_actual, x(f.c_origen),
        text(fill: gris, f.anterior), f.actual, f.origen, text(fill: gris, f.pct),
      )
    }
  },
  table.hline(stroke: 1pt + acento),
  table.cell(colspan: 8, [#text(weight: "bold")[TOTAL] #h(1em) #text(fill: gris)[Presupuesto: #d.presupuesto €]]),
  text(weight: "bold", d.anterior), text(weight: "bold", d.actual), text(weight: "bold", d.origen),
  text(weight: "bold", d.pct),
)
#if d.excesos > 0 [
  #text(fill: rojo, size: 7.5pt)[En rojo: #d.excesos partida(s) con más cantidad a origen que la presupuestada.]
]

#v(0.8em)
#block(breakable: false)[
#grid(columns: (1fr, 12cm), column-gutter: 1.2cm,
  [
    #text(fill: gris, size: 7.5pt, d.precios)
    #v(2.5em)
    #grid(columns: (1fr, 1fr), column-gutter: 1.2cm, align: center,
      [
        #text(fill: gris)[Conforme] \
        #v(3.2em)
        #line(length: 85%, stroke: 0.4pt)
        #if d.cliente != "" [#d.cliente] else [La propiedad / Dirección facultativa] \
        #text(fill: gris, size: 7pt)[Firma, fecha y sello]
      ],
      [
        #text(fill: gris)[La empresa] \
        #v(3.2em)
        #line(length: 85%, stroke: 0.4pt)
        #d.empresa \
        #text(fill: gris, size: 7pt)[Firma y sello]
      ],
    )
  ],
  table(
    columns: (1fr, auto, auto, auto),
    stroke: none,
    inset: (x: 4pt, y: 3pt),
    align: (left, right, right, right),
    table.header(text(fill: gris)[Resumen], text(fill: gris)[Anterior], text(fill: gris)[Este mes], text(fill: gris)[A origen]),
    table.hline(stroke: 0.4pt + gris),
    ..for r in d.resumen {
      let s(x) = if r.fuerte { strong(x) } else { x }
      (s(r.concepto), s(r.anterior), s(r.actual), s(r.origen))
    },
    ..if d.retencion != none { (d.retencion.concepto, [], d.retencion.importe, []) },
    table.hline(stroke: 1pt + acento),
    text(weight: "bold", fill: acento)[A COBRAR ESTE MES], [],
    text(weight: "bold", fill: acento, size: 10pt)[#d.a_cobrar €], [],
  ),
)
]
