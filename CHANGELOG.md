# Registro de cambios

Formato basado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/);
versionado [semántico](https://semver.org/lang/es/).

## [Sin publicar]

### Añadido
- **Coste real** (Obra → Coste real): se apuntan los gastos (facturas de
  material y subcontratas, horas de mano de obra con su €/h, otros) con
  proveedor, documento, capítulo y, si se quiere, el pedido. Por capítulo y
  por tipo, previsto frente a real, % gastado frente a % certificado (en rojo
  si se gasta por delante de lo certificado) y margen real a fecha
  (certificado − gastado). En Excel con la lista de gastos.
- **Pedidos registrados** (Obra → Pedidos): desde Informes → Pedido a
  fabricante o subcontrata, «Registrar en la obra» lo guarda con número,
  fecha, referencia, entrega y estado (borrador, enviado, anulado). En
  borrador se ajustan las cantidades; enviado, se anota lo recibido de cada
  línea («Recibido todo»). PDF y Excel del pedido registrado. Un pedido nuevo
  al mismo proveedor propone solo lo que falta por pedir.
- **Control de compras** (Obra → Control de compras): por recurso,
  presupuestado, necesario para lo certificado, pedido, recibido, por pedir y
  por recibir; en rojo lo pedido por encima del presupuesto.
- **Aviso de obra abierta en otro sitio**: al abrir una obra que otra persona
  (u otra ventana) tiene abierta, el programa dice quién, en qué equipo y
  desde cuándo, y pregunta antes de abrirla. Se usa un pequeño fichero
  `<obra>.abierta` junto a la obra, que se renueva cada minuto y se quita al
  cerrarla; uno de un programa cerrado de golpe se ignora a los 5 minutos.

### Cambiado
- La pantalla principal se ha dividido en módulos (árbol, ficha de partida,
  pestañas, menús, archivo, acciones) y sus pruebas van aparte. Nueva prueba
  que pinta la ventana entera en todas las vistas con presupuestos grandes.

## [0.1.0-alpha.37] - 2026-11-01

### Añadido
- **Actualización con un clic**: al arrancar, el programa mira si hay versión
  nueva en GitHub (sin enviar ningún dato) y muestra «⏶ Versión … disponible»
  en la barra de arriba. «Actualizar ahora» guarda la obra si hace falta,
  descarga el instalador, cierra el programa, lo instala y lo vuelve a abrir.
  También en **Ayuda → Buscar actualizaciones…**. Con la variable de entorno
  `PPTO_SIN_ACTUALIZAR` no se consulta nada.
- **Archivo → Importar → Modelo BIM (IFC o Revit)…**: mediciones de las
  instalaciones de un modelo, de cualquier especialidad (tuberías y sus
  accesorios, conductos, bandejas, cables, difusión de aire, valvulería,
  equipos, sanitarios, PCI, control, electricidad, iluminación). Un capítulo
  por clase (o por sistema y clase), una partida por tipo y medida (Ø o
  sección), en metros o unidades, con una línea de medición por planta y
  sistema. Sale sin precios, para valorar como un ciego. **Crear obra** o
  **Añadir a esta obra** (se puede deshacer).
  - **IFC** (2x3, 4 y 4.3): tipos, plantas (también de elementos dentro de
    espacios), sistemas, cantidades base y propiedades de longitud y tamaño,
    también las que Revit exporta en español; unidades en mm, cm o m.
  - **Revit `.rvt` sin Revit**, con rvt-rs (Apache-2.0): clase, familia y
    tipo; longitud y diámetro sacados de la geometría (sin sistemas). Fiable
    en Revit 2023–2025.
- `ppto modelo <obra.ifc|obra.rvt> [--bc3 salida.bc3] [--por-sistema] [--por-planta]`.

## [0.1.0-alpha.36] - 2026-10-31

### Añadido
- **Menú Obra → Contradictorios**: unidades o precios no previstos como
  capítulos `CTR-01`, `CTR-02`… (o marcando un capítulo existente), con
  fecha, motivo y estado (pendiente, aprobado, rechazado). Resumen del
  contrato: contratado + aprobados = vigente, y pendientes aparte. PDF de
  cada contradictorio con recuadro «Conforme» para la firma del cliente.
- **Menú Obra → Certificaciones**: lo ejecutado cada mes por partida,
  tecleando lo del mes o a origen (botón «100 %»), con anterior, mes y
  origen, % ejecutado y aviso en rojo si se supera lo presupuestado. A
  precios de venta (instaladora) o de ejecución con GG y BI en el resumen.
  Al **cerrar**, los precios quedan congelados; solo se reabre la última.
  Relación valorada en **PDF** (con IVA, retención, «a cobrar» y firmas) y
  en **Excel**.
- **Retención de garantía** (pestaña Resumen), guardada con la obra.
- Contradictorios y certificaciones se guardan en el fichero de la obra y
  en cada revisión (migración 0010).

## [0.1.0-alpha.35] - 2026-10-29

### Añadido
- **Árbol con lo que falta por valorar**: en **rojo** las partidas sin
  precio y en **naranja** las de descompuesto a medias (mano de obra sin
  material o material sin mano de obra; las subcontratadas no se marcan).
  Cada capítulo toma el color de lo peor que contiene, para encontrarlo
  aunque esté plegado. Encima del árbol, el recuento («■ 12 sin valorar ·
  ■ 5 incompletas»); al pasar el ratón, el motivo. También en la búsqueda.
- **F5 abre renglón**, como en Presto 8.8: con el ratón sobre una partida
  del árbol, partida nueva **encima** de ella (sobre un capítulo, al final
  de él); con el ratón sobre una fila del descompuesto, línea nueva encima
  de esa fila. También con clic derecho → «Insertar partida aquí» y con el
  botón «+» de cada fila del descompuesto. Un solo «Deshacer» lo quita.

- **Guardado automático**: cada minuto, si hay cambios sin guardar, se
  escribe una copia de recuperación (no crea revisiones), y también al cerrar
  la ventana. Si el programa se cierra de golpe, al abrirlo de nuevo pregunta
  si quiere **recuperar** el trabajo. Al guardar, la copia se borra.

### Corregido (revisión de integridad de datos)
- **Partidas de precio directo** (sin descomponer) faltaban en Recursos, en
  la explosión, en los pedidos y en el coste de obra. Ahora salen en su
  propio grupo («Partidas de precio directo») y en la columna «Sin
  descomponer» del coste de obra.
- Con **factores** en el descompuesto (p. ej. 0,1 × 2,002), las cantidades
  de recursos usaban la cantidad redondeada y el precio la exacta (como
  Presto): la explosión, los pedidos y las horas podían desviarse. Ahora
  usan la exacta.
- **GG, BI e IVA** se guardan con la obra: al volver a abrirla ya no vuelven
  a 13/6/21. También se recuerda el **Excel del ciego**, para «Devolver
  valorado» sin tener que buscarlo.
- El **BC3** avisa si la obra usa opciones de costes indirectos o de
  redondeo distintas de las de Presto (no caben en el formato), con el PEM
  que saldrá en Presto.
- Los saltos de línea de los textos de un BC3 se leen como en el resto del
  programa.

### Pruebas
- Generador de **presupuestos aleatorios repetibles** y pruebas de ida y
  vuelta con cientos de ellos: fichero de la obra (y revisiones), BC3, copiar
  y pegar entre obras y JSON; explosión, pedidos, mano de obra y coste
  cuadran con el PEM.
- Pruebas de punta a punta de la herramienta `ppto` y medición de la
  **cobertura** en cada PR.

## [0.1.0-alpha.34] - 2026-10-29

### Añadido
- Menú **Informes** con un desplegable de cuatro informes:
  - **Pedido a fabricante**: el material de cada fabricante (por su tarifa
    o por la oferta de su precio) con cantidad total, PVP, descuento, neto,
    oferta y partidas que lo usan. En **PDF** listo para enviar (con los
    datos de la empresa del perfil de PDF y los del fabricante de la
    Biblioteca) o en **Excel**; también todos en un Excel, una hoja por
    fabricante. Lo que no tiene proveedor sale aparte, en naranja.
  - **Pedido a subcontrata**: lo mismo para las subcontratas.
  - **Mano de obra prevista**: horas y jornadas por oficio, en toda la obra
    y por capítulo.
  - **Coste de la obra**: material, mano de obra, maquinaria, subcontratas
    y otros por capítulo, frente a la venta, con beneficio y margen.

### Corregido
- **Importar ciego** acepta el Excel de **solo mediciones** de Presto
  (Código, Nat, Ud, Resumen, Comentario, Cantidad, CanPres), sin columna de
  precio: lee las líneas de medición, separa el resumen del texto cuando
  vienen en la misma celda y, al devolverlo valorado, añade las columnas
  PrPres e ImpPres (con fórmula) detrás de las del cliente.

## [0.1.0-alpha.33] - 2026-10-28

### Corregido
- En Windows, guardar la bandeja de propuestas o la biblioteca podía fallar
  con «Acceso denegado» si otro proceso (el programa y la IA a la vez,
  OneDrive o el antivirus) tenía el fichero abierto un instante: ahora se
  reintenta.

## [0.1.0-alpha.32] - 2026-10-28

### Añadido
- **Origen del precio**: cada recurso puede vincularse a la oferta que
  justifica su precio (fabricante o proveedor, nº, fecha, validez, artículo,
  neto ofertado y **documento**). En **Recursos**, columna «Origen del
  precio»: 🖹 abre el documento, el texto sale en naranja si la oferta ha
  vencido y «≠» avisa si el precio ya no es el ofertado.
- El documento se guarda con la **ruta que se elija en cada caso** (carpeta
  de la obra, unidad de red, OneDrive…), sin copiarlo. Si se mueve, el botón
  pasa a «🖹 ?» y permite volver a vincularlo.
- **Biblioteca → Ofertas y datos de fabricantes**: lista de ofertas con su
  documento, alta de ofertas y datos del fabricante (razón social, CIF,
  contacto, teléfono, email, condiciones). Al vincular un recurso se puede
  elegir la oferta y el artículo de la biblioteca y aplicar su neto.
- La IA guarda la ruta del documento al registrar una oferta (`documento`).

### Corregido
- Se quita el símbolo ⚠, que la fuente no tiene y salía como un cuadro.

## [0.1.0-alpha.31] - 2026-10-28

### Añadido
- **Copiar y pegar entre obras**, como en Presto: con dos ventanas abiertas
  (**Archivo → Abrir en otra ventana…** o **Nueva ventana**), **Ctrl+C** copia
  la partida elegida (o clic derecho → Copiar, también capítulos enteros) y
  **Ctrl+V** la pega en el capítulo elegido de la otra obra, con su
  descompuesto, recursos, medición y descuento de fabricante. Se puede deshacer.
  - Lo que ya existe igual se reutiliza; una partida o capítulo con el mismo
    código y distinto contenido entra como `CÓDIGO_1`; un recurso con el mismo
    código y otro precio usa el de la obra de destino (y se avisa).

## [0.1.0-alpha.30] - 2026-10-27

### Añadido
- **Archivo → Comparar con…** otra revisión (R1, R2…) o un fichero BC3 (p. ej.
  la versión del cliente): partidas nuevas, quitadas y cambiadas, cantidades y
  precios antes y ahora, diferencia de cada partida y del PEM (en € y %).
- **Archivo → Nuevo desde plantilla**: Climatización (RITE), Ventilación de
  garaje (CTE, UNE 100166) y Protección contra incendios (RIPCI). Cada una con
  capítulo 00 de observaciones (criterio de valoración, exclusiones,
  condiciones), capítulos y partidas habituales con texto descriptivo (sin
  precio), legalización y capítulo 99 de contradictorios.

## [0.1.0-alpha.29] - 2026-10-27

### Añadido
- Vista **Mediciones** (Ctrl+4): todas las hojas de medición seguidas, con el
  total de cada partida y búsqueda. Comentario, Uds, Largo, Ancho y Alto se
  editan en la tabla; las fórmulas y subtotales, en la ficha.
- Vista **Recursos**: **horas por oficio** (horas, jornadas de 8 h e importe)
  y **compras por fabricante** (referencias, importe a PVP, neto y descuento
  medio).

### Cambiado
- Atajos de vistas: Ctrl+1 ficha, Ctrl+2 presupuesto, Ctrl+3 precios,
  Ctrl+4 mediciones, Ctrl+5 recursos, Ctrl+6 subcontratar, Ctrl+7 resumen.

## [0.1.0-alpha.28] - 2026-10-27

### Añadido
- **Vistas de la obra** (pestañas, menú **Ver** y atajos **Ctrl+1 … Ctrl+6**):
  - **Presupuesto**: todo el presupuesto en una tabla con capítulos plegables
    (Código · Ud · Resumen · Cantidad · Precio · Importe) y el PEM al pie. La
    cantidad y el precio directo se editan en la celda; los precios a 0 en
    rojo; clic en el código abre la ficha de la partida.
  - **Precios y márgenes**: todas las partidas con coste, sobrecoste, venta,
    margen sobre venta e importe; ordenable por columnas, búsqueda y filtros
    «solo sin precio» y «margen por debajo de x %». El sobrecoste de cada
    partida se cambia en la propia tabla, con los totales y el margen global
    arriba.

## [0.1.0-alpha.27] - 2026-10-26

### Añadido
- **Archivo → 📄 PDF / Imprimir…** a primer nivel, con el atajo **Ctrl+P**.

### Corregido
- El resumen y la unidad de un recurso (mano de obra, material…) se pueden
  escribir y cambiar después de crearlo, en la tabla del descompuesto y en la
  pestaña Recursos. Antes, un recurso creado sin texto se quedaba sin él.

## [0.1.0-alpha.26] - 2026-10-26

### Cambiado
- Barra superior agrupada en menús: **Archivo** (nuevo, abrir, guardar,
  importar, exportar, ejemplo, salir), **Edición** (deshacer, rehacer,
  cortar/copiar/pegar la partida elegida, buscar), **Precios** (tarifas,
  biblioteca, bases de precios, legalización), **IA** (asistente, propuestas),
  **Ver** (claro/oscuro, informe de importación) y **Ayuda** (versión,
  novedades). A la vista quedan deshacer/rehacer, guardar, asistente,
  propuestas, revisión y PEM.

### Añadido
- Atajos: Ctrl+N nuevo, Ctrl+O abrir, Ctrl+S guardar, Ctrl+Mayús+S guardar
  como, Ctrl+F buscar partida (además de Ctrl+Z / Ctrl+Y).

## [0.1.0-alpha.25] - 2026-10-26

### Añadido
- Asistente: **varias conexiones de IA guardadas** (p. ej. «Claude», «ChatGPT
  trabajo», «Ollama local»), cada una con su servicio, clave, modelo y precios.
  Se elige con el desplegable **IA:** del panel en modo directo; en Ajustes se
  guardan con «Guardar como conexión» y se borran con «Borrar». Los ajustes de
  versiones anteriores pasan a ser la primera conexión.

## [0.1.0-alpha.24] - 2026-10-26

### Añadido
- Asistente en modo directo: además de Claude (Anthropic), **ChatGPT (OpenAI)**
  y cualquier servicio **compatible con OpenAI** (Gemini, Mistral, Ollama o
  LM Studio en el propio equipo). Mismas herramientas y mismas reglas: la IA
  solo hace propuestas. Al pegar la clave se elige el servicio solo
  (`sk-ant-…` Claude, `sk-…` ChatGPT) y avisa si no cuadra.
- Árbol: clic derecho en el nombre de la obra o en el espacio libre →
  «Nuevo capítulo» (y «Pegar aquí» en el nivel principal). Soltar un
  subcapítulo en el espacio libre lo lleva al nivel principal.

### Cambiado
- Se quita el botón «+ Capítulo»: todo se hace con el clic derecho.
- Mensaje claro cuando la clave de la API es de otro servicio (antes solo
  «invalid x-api-key»).

## [0.1.0-alpha.23] - 2026-10-26

### Corregido
- Importar BC3: los códigos que solo se diferencian por espacios al principio
  (« TLIT15.20» y «TLIT15.20», que Presto trata como conceptos distintos) ya no
  se mezclan. El que lleva espacios pasa a llamarse «TLIT15.20_1» y se avisa en
  el informe de importación. Antes aparecían referencias circulares falsas y
  se perdía uno de los dos conceptos.

## [0.1.0-alpha.22] - 2026-10-25

### Añadido
- **Bases de precios** (botón «Bases de precios…»): se añaden BC3 de bases de
  precios (propias, de fabricante o distribuidor; se guarda una copia en la
  carpeta de datos del usuario), se busca por código o texto y se filtra por
  tipo (mano de obra, material, maquinaria, subcontrata, partidas).
  - **Copiar a la obra**: los conceptos marcados (las partidas con su
    descompuesto) quedan en la obra sin colocar en el árbol, disponibles en
    «+ Añadir línea…». Lo que ya existe en la obra no se toca.
  - **Copiar y añadir a la partida**: con una partida elegida en el árbol,
    los recursos marcados entran como líneas suyas con cantidad 1.
  - Se puede deshacer.

## [0.1.0-alpha.21] - 2026-10-25

### Añadido
- Botón **Info** en la barra de herramientas (junto a Deshacer/Rehacer): versión
  instalada, carpeta del programa y de los datos, y enlace a las novedades.
  La versión también aparece en el título de la ventana.
- Prueba del campo «Título» del menú del capítulo pinchando en él como lo
  haría el ratón (pulsar y soltar en fotogramas distintos).

## [0.1.0-alpha.20] - 2026-10-25

### Corregido
- Árbol: al pinchar en el campo «Título» del clic derecho de un capítulo, el
  menú se cerraba. Ahora se queda abierto para escribir; se cierra al pulsar
  una opción, Enter, «Guardar» o fuera del menú.

## [0.1.0-alpha.19] - 2026-10-25

### Añadido
- Árbol: el clic derecho en un capítulo o subcapítulo permite cambiar su
  título (escribir y Enter o «Guardar»). Antes, un subcapítulo creado sin
  título no se podía titular después.

## [0.1.0-alpha.18] - 2026-10-25

### Corregido
- Árbol: en la alpha.17 el clic en el título de un capítulo ya no lo abría ni
  lo cerraba (la zona para arrastrarlo se quedaba el clic). Ahora el título
  abre y cierra con un clic y se arrastra manteniendo pulsado.

## [0.1.0-alpha.17] - 2026-10-25

### Añadido
- Árbol: **Cortar**, **Copiar** y **Pegar aquí** en el menú del clic derecho
  para mover o copiar partidas a otro capítulo o subcapítulo. Copiar pone la
  misma partida (mismo código y precio, como Presto) con su cantidad y su
  medición; la cantidad de cada capítulo es independiente.
- Se puede soltar una partida arrastrada dentro de un subcapítulo vacío, no
  solo sobre su título.
- Los capítulos y subcapítulos también se arrastran: se coge el título y se
  suelta sobre el título de otro capítulo para meterlo dentro.

### Corregido
- Instalador: corregido un error de sintaxis de la alpha.16 que impedía
  ejecutar `instalar.ps1` («La referencia de variable no es válida»).

## [0.1.0-alpha.16] - 2026-10-21

### Corregido
- BC3 con descuentos a la vista: la línea «Descuento FABRICANTE −x %» se
  exporta con la naturaleza del recurso descontado (material, mano de obra o
  maquinaria). Antes salía como «Otros» y en Presto restaba en OtrPres en
  lugar de en MatPres.
- Instalador: espera a que el programa se cierre del todo y reintenta la
  copia durante unos segundos. Antes podía fallar con «el archivo está siendo
  utilizado por otro proceso» aunque el programa ya se hubiera cerrado.

## [0.1.0-alpha.15] - 2026-10-20

### Corregido
- Presupuesto en PDF: los datos de la obra (título, dirección, cliente y
  referencia) se guardan por obra y se recuperan al volver a abrir la
  ventana o el programa. Antes se vaciaban cada vez.

## [0.1.0-alpha.14] - 2026-10-20

### Corregido
- Ciego en Excel: se aceptan las exportaciones de Presto sin mediciones,
  con cabecera «Pres» (en lugar de «PrPres») y filas «Total CÓDIGO» en la
  columna Resumen (en lugar de Parcial). Antes no se encontraba la cabecera
  o los capítulos quedaban anidados mal.

## [0.1.0-alpha.13] - 2026-10-20

### Cambiado
- BC3: las subcontratas se exportan como **material (tipo 3)** en lugar de
  tipo 0. Presto 8.8 tomaba el tipo 0 sin descompuesto por una unidad de
  obra; como material suma en el material y basta marcarla «Subcontratado».

## [0.1.0-alpha.12] - 2026-10-20

### Corregido
- BC3 con descuentos a la vista: cuadra **al céntimo** con la obra. Aplicar
  el descuento al importe de la línea (como hace Presto) en vez de al precio
  unitario movía algún céntimo por partida (6,73 € en una obra real); ahora
  se añade una línea «Ajuste de redondeo de descuentos» (AJ.REDONDEO,
  0,01 € × céntimos) en las partidas que lo necesitan. Al reimportar el BC3
  el ajuste se quita junto con las líneas de descuento.

## [0.1.0-alpha.11] - 2026-10-19

### Añadido
- Al importar un BC3 exportado con los descuentos a la vista (recurso a PVP
  + línea «Descuento FABRICANTE x %»), se recuperan la tarifa del recurso y
  el descuento del fabricante, y se quitan esas líneas: la obra vuelve a
  tener sus «Descuentos por fabricante» editables. El importe no cambia.

## [0.1.0-alpha.10] - 2026-10-19

### Quitado
- «Precio de venta a partir del PEM» en «Resumen y venta»: era una
  calculadora que no cambiaba ningún precio. El margen se ve y se fija
  partida a partida (sobrecoste, margen sobre venta, venta bloqueada).

### Cambiado
- Legalización (rol instalador): la tarifa de comparación usa el sobrecoste
  de la obra (GG + BI) en lugar de la calculadora quitada.

## [0.1.0-alpha.9] - 2026-10-19

### Añadido
- **Venta bloqueada por partida**: «Venta bloqueada €/ud» o el botón
  «🔒 Bloquear en …»; la partida se vende a ese importe aunque cambie su
  coste (sobrecoste y margen se recalculan; en rojo si se pierde dinero).
  Se ve con 🔒 en la tabla y sale en el PDF.
- **Sobrecoste y margen sobre venta** en cada partida y en la tabla de
  «Resumen y venta» (y el total de la obra).
- **Sobrecoste propio por partida.** En la pestaña Partida, «Sobrecoste de esta
  partida %» sale relleno con el margen de la obra (GG + BI); si se escribe
  otro, esa partida tiene el suyo (venta = precio × (1 + margen)) y un botón
  la devuelve al de la obra. Se ve en el desglose, en
  la columna «Margen %» de la tabla de «Resumen y venta» (en negrita los
  propios) y en el PDF de instaladora. Se guarda con la obra y se puede
  deshacer.

## [0.1.0-alpha.8] - 2026-10-19

### Añadido
- **Coste y venta partida a partida.** En la pestaña Partida, debajo del
  precio, el desglose completo por unidad y por importe: mano de obra,
  material, subcontrata, maquinaria y otros → coste directo → costes
  indirectos → PEM → gastos generales → beneficio industrial → **venta sin
  IVA**, con el margen de la partida. En «Resumen y venta», tabla de todas las
  partidas con coste y venta (unitarios e importes); al pulsar un código se
  abre su desglose.
- **PDF de instaladora con precios de venta**: casilla «Precios de venta en
  cada partida» (marcada por defecto en el perfil Instaladora). Cada partida
  sale a PEM + GG + BI con los porcentajes de «Resumen y venta», igual que su
  desglose en pantalla, y capítulos y total suman esos importes. Antes el PDF
  de instaladora imprimía el precio de ejecución material (el coste si no hay
  costes indirectos).
- Exportar BC3 para **Presto antiguo** (menú «📤 Exportar BC3» → «Presto
  antiguo (códigos de 12 caracteres)»; MCP `exportar_bc3` con
  `longitud_codigo`). Las versiones antiguas cortan los códigos al importar y
  juntaban conceptos distintos (p. ej. un tubo y su línea de descuento, o dos
  rejillas de la misma serie), cambiando el precio de la obra. Ahora los
  códigos largos se renombran a códigos únicos y cada línea de descuento
  sigue a su recurso; el importe no cambia.

## [0.1.0-alpha.7] - 2026-10-19

### Corregido
- MCP `proponer`: `nueva` se acepta también como texto JSON (clientes con el
  esquema de herramientas anterior lo envían así).

## [0.1.0-alpha.6] - 2026-10-19

### Añadido
- Contradictorios y adicionales: una propuesta puede traer `nueva` (capítulo,
  unidad, resumen, texto y cantidad) para una partida que no está en el
  presupuesto; al aceptarla se crea la partida y, si falta, su capítulo detrás
  del indicado. MCP `proponer` y `proponer_lote` aceptan `nueva`; la tarjeta
  de la propuesta la marca como «NUEVA en …».

## [0.1.0-alpha.5] - 2026-10-19

### Añadido
- Descuentos a la vista: al exportar a BC3, cada material de tarifa sale a
  su PVP con una línea porcentual negativa «Descuento FABRICANTE x %» detrás
  (máscara = código del recurso), para que Presto muestre PVP y descuento.
  El importe puede variar algún céntimo por el redondeo de la línea. En la
  ventana, bajo el resumen del material se ve «PVP … − x % FABRICANTE».

## [0.1.0-alpha.4] - 2026-10-18

### Añadido — biblioteca de precios (paso 1)
- Crate `ppto-biblioteca`: partidas con descompuesto y versiones; por
  fabricante, descuentos (general y por grupo), tarifas por año con
  comparación y aviso de tarifa antigua, subida estimada y ofertas en netos
  con descuento real. Fichero propio, opcionalmente compartido.
- MCP: `buscar_biblioteca`, `estado_biblioteca`, `guardar_en_biblioteca`,
  `registrar_tarifa`, `registrar_oferta`, `fijar_descuento_biblioteca`. La
  IA consulta la biblioteca antes de estimar.
- Interfaz: ventana «Biblioteca…» (guardar la obra, fabricantes, cargar
  tarifa de un año, descuentos y búsqueda de partidas).

### Añadido — legalización
- Crate `ppto-legalizacion` y ventana «Legalización…» (RITE o PCI, memoria o
  proyecto, terciario o viviendas): una partida por disciplina en el
  capítulo `LEG` con todo el descompuesto. La tarifa sale de nuestros
  costes: horas por tarea (memoria, tramitación y dirección, redacción,
  justificaciones RITE y RSIF, visitas a obra y de la OCA) por el coste de
  cada perfil, desplazamientos (horas, km, aparcamiento, dietas) y margen
  según el tamaño del encargo; tasas, EICI y visados se repercuten sin
  margen. Los honorarios en % del PEM quedan como referencia de mercado.
- RITE art. 15: documento exigido según la suma de potencias de calor o de
  frío (proyecto por encima de 70 kW).
- Avisos de proyecto o memoria en PCI (DB SI 4) y frigoríficas (RSIF nivel 2).
- «Rellenar desde el presupuesto»: propone potencias, circuitos RSIF,
  refrigerante, instalación frigorífica, sistemas PCI, BIE y detección a
  partir de las mediciones, con la partida de la que sale cada dato.
- Baremo por año en `legalizacion.json`, fuera del repositorio; costes,
  horas y margen se editan en la ventana.

## [0.1.0-alpha.3] - 2026-10-16

### Añadido
- MCP `proponer_lote`: hasta 40 propuestas por llamada, con los recursos
  comunes definidos una vez, líneas abreviadas y medios auxiliares
  automáticos. Valorar una obra entera pasa de una llamada por partida a
  unas pocas.

### Corregido
- BC3 ciegos de Presto: las unidades de obra sin tipo (0) y sin
  descompuesto que cuelgan de un capítulo se importan como partidas (antes
  como «otros», y la IA no podía proponer su precio). Las obras ya abiertas
  se corrigen al aceptar la propuesta.

## [0.1.0-alpha.2] - 2026-10-15

### Añadido — instalador para Windows
- Cada versión publica `presupuestos-bc3-<versión>-windows.zip` con
  `ppto-gui.exe`, `ppto-mcp.exe`, `ppto.exe` e `instalar.bat`: instala para
  el usuario sin administrador, crea accesos directos y configura Claude
  Desktop (con copia de seguridad) si está instalado.

### Añadido — panel del asistente
- **✨ Asistente**: conversación con la IA por obra, con adjuntos (ofertas,
  tarifas, mediciones) e informes. Modo *encargo por MCP* (gratis: «atiende
  el asistente» en Claude Desktop) o *directo* con clave de API, en segundo
  plano, con coste por respuesta y límite de gasto mensual.
- MCP: `leer_mensajes`, `responder`, `leer_adjunto` (Excel, PDF, CSV) y
  `guardar_informe`; `herramientas_api` y `ejecutar` para el modo directo.
- Nuevo crate `ppto-asistente` (bucle de herramientas con la API de
  mensajes, caché de instrucciones, reintentos, control de gasto).
- «Reaplicar aceptadas»: vuelve a aplicar las propuestas aceptadas al abrir
  de nuevo el Excel ciego.

### Corregido
- La bandeja de propuestas se podía perder cuando el programa y el MCP la
  escribían a la vez o fallaba una lectura: ahora se modifica con fichero de
  bloqueo y no se sobrescribe si no se puede leer.

### Cambiado
- MCP `proponer`: solo admite descompuestos (material, mano de obra,
  maquinaria o subcontrata y medios auxiliares); los precios directos se
  rechazan.

### Añadido — descuentos por fabricante
- Recursos con **PVP de tarifa y fabricante** (`PrecioTarifa`): el neto es
  PVP × (1 − descuento del fabricante). `Presupuesto::descuentos`,
  `fijar_descuento` (recalcula todos los recursos del fabricante),
  `fijar_tarifa`, `fabricantes`. Escribir el neto a mano desliga la tarifa.
- Base de datos: migración 0004 (PVP, fabricante y tabla de descuentos por
  revisión). El BC3 sigue llevando netos.
- Propuestas de la IA: líneas con `pvp` y `fabricante` y descuentos
  estimados por fabricante, que solo se usan si la obra no tiene uno.
- Interfaz: Recursos → «Descuentos por fabricante» (editar o añadir) y
  columna de tarifa con el PVP editable.
- **Descuentos por grupo de producto** (p. ej. S&P E08): clave «S&P/E08»;
  un recurso con grupo usa el descuento del grupo si existe y, si no, el
  general del fabricante. Grupo editable en Recursos; quitar un descuento
  de grupo devuelve sus recursos al general. Migración 0005.

### Añadido — propuestas de la IA con aprobación (P-021)
- `ppto_core::propuesta`: propuesta de descompuesto o precio directo para
  una partida, con confianza y justificación; `aplicar_propuesta` (reutiliza
  recursos existentes con su precio, da de alta los nuevos, todo o nada) y
  `simular_propuesta`.
- `ppto-propuestas`: obra activa publicada por el programa y bandeja de
  propuestas compartida con el MCP (escritura atómica).
- MCP: `abrir_obra_activa`, `partidas_sin_precio`, `buscar_tarifas`,
  `proponer` y `ver_propuestas`. Ninguna herramienta modifica el presupuesto.
- Interfaz: «✨ Propuestas IA (n)» con vista previa del precio e importe,
  descompuesto, justificación, Aceptar/Rechazar, aceptar en bloque las de
  confianza alta y Deshacer (la propuesta vuelve a pendiente).

### Añadido — coeficiente de dificultad
- `Presupuesto::aplicar_dificultad(ámbito, coeficiente)`: multiplica los
  rendimientos de mano de obra propia de toda la obra, un capítulo o una
  partida (y de sus auxiliares). No toca el precio de la hora, materiales,
  maquinaria ni subcontratas; los % se recalculan. Partidas o auxiliares
  compartidos con otros capítulos no se tocan y se informan. Informe de
  horas y coste antes/después.
- Interfaz: pestaña Recursos → «Coeficiente de dificultad», con vista previa,
  motivo y Deshacer. Se anota en la auditoría de la revisión al guardar
  (`Almacen::anotar`).

### Añadido — modo claro / oscuro
- Interfaz: botón «☀ Claro» / «🌙 Oscuro» en la barra; la elección se guarda
  en `tema.txt` de la carpeta de configuración (sin elección, sigue al
  sistema). Colores de tipo de recurso legibles en los dos modos.

### Añadido — tarifas de subcontratas
- `ppto-tarifas`: lee la tarifa de una subcontrata en Excel (Referencia,
  Descripción, Precio y, si la hay, Unidad; proveedor y año del título).
  Sin columna de unidad, la deduce de la descripción («/ METRO», «(1H)»,
  cable, tubo…) y marca las dudosas para revisar. Biblioteca de tarifas en
  `tarifas.json` de la carpeta de configuración (nunca en el repositorio),
  búsqueda en todas y alta como recurso de subcontrata `PROVEEDOR.REFERENCIA`.
- Interfaz: «Tarifas…» (importar, revisar unidades y precios, guardar) y
  «+ Añadir línea… → De tarifa de subcontrata».
- CLI: `ppto tarifa <tarifa.xlsx>` lista los recursos con su unidad.

### Añadido — subcontrata a ratio
- `ppto_core::ratio`: `Presupuesto::aplicar_ratio(PaqueteRatio)` aplica al
  presupuesto una subcontrata por unidad de contratación (p. ej. montaje VRF
  a € por máquina): medidor = suma de las cantidades de las partidas
  elegidas; en cada partida se quitan los recursos que pone la subcontrata
  y se añade la línea de subcontrata; reparto en proporción al coste propio
  retirado (Σ rendimientos × cantidades = medidor exacto); los medios
  auxiliares se conservan. Partida entera o solo mano de obra = paquete de
  una partida. Informe: importe, retirado, coste antes/después, horas.
- Interfaz: «Subcontrata a ratio» en la pestaña Subcontratar, con vista
  previa y Deshacer.

### Añadido — presupuestos ciegos
- `ppto-ciego`: importa el listado de Presto en Excel (.xlsx/.xls) y devuelve
  el mismo .xlsx con los precios (solo se escriben las celdas PrPres;
  `fullCalcOnLoad`). Mediciones a precisión completa para cuadrar al céntimo
  con el Excel; parciales fijos; números y códigos guardados como texto;
  partidas repetidas en varios capítulos. Validado con un ciego real de 507
  partidas y 1.684 líneas de medición (no se sube).
- `traer_precios` / `Presupuesto::traer_precio`: trae de otra obra o de un BC3
  el descompuesto completo (o el precio directo) de cada partida sin precio.
- Partidas sin descomponer con precio directo (como en Presto).
- `ppto_core::formato::cant`: cantidades sin ceros sobrantes.
- CLI `ppto ciego`; interfaz: menú «📋 Ciego» y precio directo en la partida.
- Interfaz: **Deshacer / Rehacer** (Ctrl+Z, Ctrl+Y) de todos los cambios del
  presupuesto, hasta 200 pasos; navegación anterior/siguiente/sin precio
  (Ctrl+↑/↓) y buscador de partidas.

### Añadido — PDF (P-016)
- `ppto-pdf`: presupuesto en PDF con Typst incluido (ADR-0007): portada con
  logo, fecha y revisión; capítulos y subcapítulos; textos justificados con
  partición silábica en castellano; mediciones; resumen con GG, BI, IVA, total
  en letra y firma; hoja de observaciones. Plantilla editable.
- `ppto_core::formato::en_letra` (importe en letra) y `fecha_larga`/`fecha_hoy`.
- Interfaz: botón «PDF…» con ventana de opciones que recuerda empresa, firma,
  logo y observaciones. CLI: `--pdf`.
- Perfiles de empresa en el PDF (`ppto_pdf::Perfiles`): instaladora (sin PEM,
  GG ni BI) e ingeniería (PEM + GG + BI) de serie, cada uno con su logo
  (copia guardada), empresa, firma y observaciones tipo; duplicar y borrar.

### Cambiado
- Versión mínima de Rust: 1.92 (requisito de Typst).

### Añadido — edición (P-014)
- `ppto-core::edicion`: `nuevo_capitulo`, `nueva_partida`, `nuevo_recurso`,
  `anadir_linea`, `quitar_linea`, `quitar_y_purgar`, `mover_linea`,
  `borrar_concepto`, `padres`, `fijar_unidad`, `codigo_libre`. Cada operación
  deja el presupuesto válido o no lo toca.
- Interfaz: botón «Nuevo», «+ Capítulo», menú contextual en el árbol, ventana
  «Añadir línea…» con buscador o recurso nuevo, ^ v x en el descompuesto y
  unidad editable.
- Subpartidas (auxiliares descompuestos) a cualquier nivel: crear desde
  «Añadir línea…», entrar pulsando su código, «<< Volver» y ruta superior;
  `Presupuesto::ruta`.
- Arrastrar y soltar partidas en el árbol entre capítulos y niveles
  (`Presupuesto::trasladar_linea`: conserva cantidad y hoja de medición;
  si no es válido, no cambia nada).

### Validado con Presto 8.8 (10-10-2026)
- **Importación de una obra real** (climatización, 208 conceptos, CI 45 %,
  descuentos y porcentajes): 52/52 partidas, 5 capítulos y PEM 157.310,88 €
  al céntimo. El fichero real no se incluye en el repositorio.
- **Exportación**: el BC3 del ejemplo abierto en Presto da 7.005,15 € y el
  mismo desglose por partidas y naturalezas.

### Corregido
- Redondeos como Presto 8.8: porcentaje = redondeo(base/100, dec. rendimiento)
  × puntos; factor × rendimiento sin redondear antes de multiplicar.
- `~C` de partidas = coste sin costes indirectos; capítulos y raíz, con ellos
  (lectura y escritura).
- BC3 que declara ANSI pero llega en UTF-8: se lee bien y se avisa; aviso si
  hay caracteres «�» (tildes perdidas antes de importar).
- Material (tipo 3) con descomposición: información, no aviso.
- CI: la comprobación DCO ignora los commits de fusión que crea GitHub.

### Añadido
- `ppto-mcp`: servidor MCP de lectura y simulación (P-020) — abrir BC3/SQLite,
  resumen, búsqueda, detalle de conceptos, recursos y horas, simulación de
  CI/precios/cantidades y de subcontratas, exportación a Excel y BC3.
  Guía en `docs/mcp.md`.
- Exportador BC3 (P-011) en el formato de Presto 8.8 (FIEBDC-3/2002, ANSI,
  CRLF) con pruebas de ida y vuelta; `--bc3 salida.bc3` en `demo` e
  `importar` y botón «Exportar BC3…».
- Texto descriptivo (`~T`) visible y editable; incluido en el Excel.
- `ppto-informes`: libro Excel con Resumen, Presupuesto (agrupado por
  capítulos), Descompuestos, Mediciones, Recursos y Horas por oficio,
  preparado para imprimir en A4. `ppto demo|importar … --excel f.xlsx` y
  botón «Excel…» en la interfaz. Cifras del motor, sin fórmulas de Excel.
- Opción «Redondear partidas que actúan como auxiliares» (Presto; desmarcada).
- Costes indirectos de obra aplicados por partida, con las opciones de
  redondeo de Presto; PEM con indirectos y coste directo por separado. Se
  leen del `~K` del BC3, se guardan en SQLite (migración 2) y se editan en
  «Resumen y venta». Recursos y subcontratas pasan a comparar con coste directo
  (`Explosion::coste_directo`, `ResultadoEscenario::coste_original/coste_escenario`).
- `ppto-bc3`: importador FIEBDC-3 (P-010) con lectura tolerante de `~V ~K ~C
  ~D ~Y ~T ~M ~N`, codificación ANSI/UTF-8, incidencias con número de línea,
  reconstrucción de la raíz, ruptura de ciclos y comparación de precios
  recalculados frente a los declarados en el BC3.
- `ppto importar <fichero.bc3> [--db …] [--todo]` y botón «Importar BC3…»
  con informe de importación en la interfaz.
- `Presupuesto::valorar`: valoración en bloque (4.000 partidas en ~70 ms).
- Líneas porcentuales con máscara FIEBDC-3 (prefijo del código antes de `%`/`&`);
  en BC3 el rendimiento del porcentaje se lee como fracción (0.03 = 3 %).
- `ppto-gui`: interfaz de escritorio básica (prototipo de P-013): árbol de
  capítulos, descompuesto y mediciones editables con recálculo inmediato,
  recursos con precios editables, comparación de ofertas de subcontrata,
  resumen PEM→PEC y precio de venta por margen; abrir y guardar revisiones
  en SQLite.
- `ppto-core`: `actualizar_medicion`, `fijar_rendimiento`, `fijar_precio` y
  módulo `formato` (lectura y escritura de números en notación española).
- `ppto-db`: `listar_revisiones` y `siguiente_etiqueta`.
- `scripts/windows/`: accesos directos para abrir la interfaz y ejecutar pruebas.

## [0.1.0-alpha.1] — 2026-10-09

### Añadido
- `ppto-core`: conceptos, precios descompuestos jerárquicos con detección de
  ciclos, líneas porcentuales, mediciones con fórmulas y subtotales,
  redondeo comercial por ámbitos, explosión de recursos con descuadre de
  redondeo, margen sobre venta y recargo sobre coste, resumen PEM→PEC,
  escenarios de subcontratación por unidad o alzado.
- `ppto-db`: esquema SQLite v1, migraciones, revisiones independientes,
  bloqueo de revisiones y auditoría.
- `ppto-cli`: `ppto demo` con el caso de validación de suelo radiante.
- Documentación de arquitectura, reglas de cálculo, ADR 0001–0005 y matriz BC3.
- Gobernanza: plantillas de Issues y PR, CODEOWNERS, CI en Linux y Windows,
  comprobación de licencias.
