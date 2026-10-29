// SPDX-License-Identifier: GPL-3.0-or-later
// Pedido a un fabricante o subcontrata (Typst). Los datos llegan en
// «datos.json» con los importes ya formateados (1.234,56).

#let d = json("datos.json")
#let acento = rgb("#1f4e79")
#let gris = luma(110)

#set document(title: d.titulo, author: d.empresa)
#set text(font: "New Computer Modern", size: 9pt, lang: "es", region: "es")
#set page(
  paper: "a4",
  flipped: d.material,
  margin: (x: 1.6cm, top: 1.6cm, bottom: 1.8cm),
  footer: context [
    #set text(size: 7.5pt, fill: gris)
    #grid(columns: (1fr, auto), [#d.titulo · #d.obra],
      [Página #counter(page).display() de #counter(page).final().first()])
  ],
)

#grid(
  columns: (1fr, auto),
  align: (left + top, right + top),
  [
    #text(size: 9pt, fill: gris, tracking: 0.12em)[#upper(d.tipo)]
    #v(0.2em)
    #text(size: 17pt, weight: "bold", fill: acento, d.proveedor)
    #v(0.2em)
    #for l in d.datos_proveedor [#text(fill: gris)[#l.at(0):] #l.at(1) \ ]
  ],
  [
    #if d.logo != none [#image(d.logo, height: 1.6cm) \ ]
    #text(weight: "bold", d.empresa) \
    #text(size: 8pt, fill: gris, d.empresa_datos)
  ],
)
#v(0.4em)
#line(length: 100%, stroke: 0.8pt + acento)
#grid(columns: (auto, 1fr), column-gutter: 1em, row-gutter: 0.45em,
  text(fill: gris)[Obra], text(weight: "bold", d.obra),
  text(fill: gris)[Fecha], d.fecha,
  ..if d.referencia != "" { (text(fill: gris)[Nuestra ref.], d.referencia) },
)
#v(0.6em)

#let cab(x) = text(weight: "bold", fill: white, size: 8pt, x)
#table(
  columns: if d.material { (auto, auto, 1fr, auto, auto, auto, auto, auto, auto) } else { (auto, 1fr, auto, auto, auto, auto) },
  stroke: (x, y) => if y == 0 { none } else { (bottom: 0.3pt + luma(200)) },
  fill: (x, y) => if y == 0 { acento } else if calc.even(y) { luma(245) } else { none },
  align: (x, y) => if y > 0 and x >= (if d.material { 4 } else { 3 }) { right } else { left },
  inset: (x: 4pt, y: 3.5pt),
  table.header(..if d.material {
    (cab[Código], cab[Ref.], cab[Descripción], cab[Ud], cab[Cantidad], cab[PVP €], cab[Dto.], cab[Neto €], cab[Importe €])
  } else {
    (cab[Código], cab[Descripción], cab[Ud], cab[Cantidad], cab[Precio €], cab[Importe €])
  }),
  ..for l in d.lineas {
    if d.material {
      (l.codigo, l.referencia, l.resumen, l.unidad, l.cantidad, l.pvp, l.descuento, l.precio, l.importe)
    } else {
      (l.codigo, l.resumen, l.unidad, l.cantidad, l.precio, l.importe)
    }
  },
)
#v(0.3em)
#align(right, text(size: 11pt, weight: "bold")[Total pedido (sin IVA): #d.total €])

#if d.condiciones != "" [
  #v(0.8em)
  #text(weight: "bold")[Condiciones] \
  #d.condiciones
]
#v(1fr)
#text(size: 7.5pt, fill: gris)[Cantidades previstas en presupuesto. Rogamos confirmen precios, plazo de entrega y disponibilidad.]
