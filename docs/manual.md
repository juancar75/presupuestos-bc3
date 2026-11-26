# Manual de presupuestos-bc3

## 1. Qué es e instalación

presupuestos-bc3 es un gestor de presupuestos de instalaciones que lee y escribe BC3 (FIEBDC-3), compatible con Presto. Cada obra se guarda en un fichero `.sqlite` con todas sus revisiones.

**Instalación (Windows)**

1. Descarga la última versión desde Releases en GitHub (`presupuestos-bc3-…-windows.zip`) o solo `ppto-gui.exe`.
2. Descomprime en una carpeta propia (por ejemplo `C:\Programas\presupuestos-bc3`) y abre `ppto-gui.exe`.
3. Para actualizar, sustituye el `.exe` (o Ayuda → «Buscar actualizaciones…»): las obras y la configuración no se tocan.

La configuración (bases, especialidades, recientes, carpeta de tarifas) vive en `%APPDATA%\presupuestos-bc3`.

**Ayuda dentro del programa**: **F1** o el botón **?** de la barra abren este manual; los botones **?** de algunas ventanas lo abren en su apartado.

## 2. Crear o abrir una obra

1. **Archivo → Nuevo…** (Ctrl+N).
2. Marca una o varias especialidades: Clima, Ventilación de garaje, PCI. Con varias, los capítulos llevan prefijo (CL, VG, PCI) para no mezclarse.
3. Elige los capítulos que entran en la oferta.
4. Rellena la ficha (cliente, obra, número de oferta) y guarda con **Guardar como…**: se crea el `.sqlite`.

**Otras formas de empezar**

- **Seguir con una reciente**: la ventana «Nuevo presupuesto» muestra los 5 últimos; Archivo → «Abrir reciente», los 10 últimos. Los que ya no existen salen en gris.
- **Abrir…** (Ctrl+O): abre un `.sqlite`; empieza en la carpeta del último abierto.
- **Importar un BC3** (Archivo → Importar): de Presto o de otro programa. Si trae los descuentos a la vista (PVP + línea de descuento), se recuperan como tarifa + descuento del fabricante.
- **Ciego del cliente**: BC3 o Excel sin precios. Al importarlo se abre sola la comprobación antes de valorar (apartado 4).
- **Abrir ejemplo**: un presupuesto sintético para practicar.

**Carpeta de documentos de la obra**: una carpeta por obra (OneDrive o red) con estas subcarpetas, para que el programa y Claude encuentren cada cosa:

| Subcarpeta | Qué va |
| --- | --- |
| 01 Proyecto | Memoria descriptiva, memoria de cálculo y pliego (PDF o Word) |
| 02 Mediciones | Ciego o mediciones del cliente (BC3 o Excel) |
| 03 Planos | Planos en DXF (mejor), DWG y PDF; modelos BIM en IFC |
| 04 Ofertas | Ofertas de proveedores y subcontratas de esta obra |
| 05 Cliente | Correos, actas, aclaraciones y contrato |

En «Datos de la obra», junto a la carpeta: **?** explica dónde va cada cosa, «Crear carpetas» crea las que falten y debajo se ve cuántos ficheros hay en cada una, con avisos (subcarpetas vacías, ficheros sueltos, DWG sin su DXF).

**Datos de la obra** (Obra → «Datos de la obra…»): obra, dirección, municipio, cliente, a la atención de, promotor, referencia, **carpeta de documentos** (planos, memorias, cálculos) y la ficha del edificio que piden sus especialidades (superficies, plantas sobre y bajo rasante, viviendas, plazas de garaje, escaleras…). Se guardan con la obra y el PDF toma de aquí la obra, la dirección y el cliente. Claude puede rellenarlos leyendo la carpeta de documentos («rellena los datos de la obra con sus documentos»): lo que propone sale en azul con su fuente («Memoria, pág. 3») y se acepta con ✔ o «Aceptar todos». Si la obra tiene carpeta de documentos y faltan datos, Claude lo hace solo al conectarse, antes de valorar; también se le puede encargar con el botón «✨ Que Claude los rellene con los documentos».

**Revisiones**: cada guardado puede ser una revisión nueva (R1, R2…) dentro del mismo fichero; las anteriores se conservan y se comparan con Archivo → «Comparar con…».

## 3. La pantalla principal

A la izquierda, el árbol de capítulos y partidas; a la derecha, las pestañas de trabajo. Arriba a la derecha, siempre el PEM.

| Pestaña | Para qué |
| --- | --- |
| Partida | Descompuesto, texto, medición, coste/venta y casillas «Excluida» / «Incluida». |
| Presupuesto | El presupuesto por capítulos, como lo verá el cliente. |
| Precios y márgenes | Coste, venta y margen de cada partida. |
| Mediciones | Líneas de medición (uds, largo, ancho, alto, fórmula). |
| Recursos | Materiales, mano de obra y subcontratas de la obra, horas por oficio y descuentos por fabricante. |
| Subcontratar | Simula pasar partidas o mano de obra a una subcontrata. |
| Resumen y venta | Indirectos, gastos generales, beneficio, IVA y total. |

**Colores**: al pasar el ratón por una partida o capítulo en rojo, naranja o tachado, el programa dice qué le pasa (sin precio, falta mano de obra, excluida…).

En las tablas, las celdas con recuadro se editan: escribe y pulsa Intro.

**Cambiar códigos**: clic derecho en un capítulo del árbol, en el código de la partida (arriba en la pestaña Partida) o en una línea del descompuesto → «Código» → escribe el nuevo y «Cambiar». Cambia en todo el presupuesto (líneas, mediciones, contradictorios y certificaciones). Clic derecho en el nombre de la obra → **Renumerar capítulos** los deja en orden: C01, C02… y los subcapítulos C01.01, C01.02… (las partidas no cambian). Las dos cosas se deshacen con Ctrl+Z.

**Elegir varias partidas** en el árbol: Ctrl + clic añade o quita una; Mayús + clic elige el bloque desde la última, dentro del mismo capítulo (Esc deja de elegir). Clic derecho sobre una de ellas: cuánto suman, **Mover a…** o **Copiar a…** otro capítulo, excluir o quitar «excluida», marcar o quitar «incluida en otra», o quitarlas. Todo se hace de una vez y se deshace con un solo Ctrl+Z; si algo falla, no se cambia nada.

**Sustituir en todo el presupuesto**: clic derecho en una línea del descompuesto (o en una partida del árbol) → «Sustituir … por…», y elija el recurso o la partida nueva (del mismo tipo). Se pone con la misma cantidad en todas las partidas donde estaba (si ya estaba, se suman las cantidades). Por ejemplo, cambiar un detector de una marca por el de otra en toda la obra.

## 4. Comprobar antes de valorar

Lo primero, antes de poner precios: **Precios → «Comprobar antes de valorar…»** (se abre sola al importar un ciego). Lee la obra y dice si están los recursos para sacar los precios:

- **Por fabricante de la obra**: si tiene tarifa (en la carpeta de tarifas o en la biblioteca), si tiene descuento (en la obra o, al menos, en la biblioteca) y si hay un nombre parecido («SALTOKI» y «SALTOKI ONLINE»): use el mismo nombre en la tarifa, el descuento y los materiales.
- **Marcas** de la biblioteca o de la carpeta que se nombran en partidas sin precio y no tienen tarifa o descuento.
- **Materiales con neto fijo** (sin PVP ni descuento a la vista), **con PVP distinto** del de la biblioteca y **recursos a 0 €**.

Los avisos van de lo más grave a lo que está bien: ✖ falta, ⚠ revisar, ✔ bien. «Copiar» pasa la lista a un correo o al chat. Claude hace la misma comprobación (`comprobar_recursos`) antes de proponer precios.

Solo reconoce marcas que ya están en la biblioteca o en la carpeta de tarifas, y no sabe que dos nombres sin parecido son el mismo fabricante.

## 5. Precios: descompuestos, tarifas y descuentos

Toda partida se valora con un descompuesto: material, mano de obra, maquinaria o subcontrata, y medios auxiliares. No se ponen precios directos.

**Tarifa y descuento (siempre PVP y descuento, nunca netos)**

- Cada material de fabricante lleva su **PVP de tarifa**, el **fabricante** y, si lo hay, el **grupo** (por ejemplo S&P/E02).
- El descuento se fija **una vez por fabricante o grupo** en la obra y se aplica a todos sus materiales: neto = PVP × (1 − descuento).
- Cambiar un descuento recalcula a la vez todas las partidas que usan ese fabricante.
- Solo van a precio directo los materiales sin tarifa (soportes propios, bandas, impuestos).

**Líneas porcentuales**

Una línea en % (por ejemplo `%AC30` accesorios, `%MA` medios auxiliares) se aplica sobre la suma de las líneas **que tiene encima**. El orden importa: si el 30 % de accesorios solo debe aplicarse al tubo, el tubo va primero y el `%AC30` justo debajo.

Ejemplo sintético, 1 m de tubería:

| Línea | Cálculo | € |
| --- | --- | --- |
| Tubo | PVP 10,00 − 50 % | 5,00 |
| %AC30 accesorios | 30 % de 5,00 | 1,50 |
| Aislamiento | 1,1 m × 1,00 | 1,10 |
| Mano de obra oficial | 0,20 h × 30,00 | 6,00 |
| %MA | 2 % de lo anterior | 0,27 |
| **Total** |  | **13,87 €/m** |

**Partida excluida o incluida**

- **Excluida**: lo que hacen terceros. Se queda en el presupuesto, tachada, con importe 0, y en el PDF y el Excel sale «EXCLUIDO».
- **Incluida**: conserva medición y descompuesto pero no suma en el capítulo; sale «INCLUIDO» (por ejemplo, legalizaciones de un expediente único).

## 6. Valorar a mano rápido

**Buscador único** («+ Añadir línea…», **Ctrl+L** o **Insert**): busca a la vez en lo que ya está en la obra, en los artículos de las tarifas de la biblioteca (con PVP − descuento = neto), en los kits y en las tarifas de subcontrata. Palabras sueltas y sin tildes («vfc 250»); **↑/↓** para elegir, **Intro** o doble clic para añadir.

**Atajos**

| Tecla | Qué hace |
| --- | --- |
| Ctrl+L / Insert | Añadir línea (buscador) |
| Ctrl+D | Copiar el descompuesto de la partida anterior |
| Ctrl+Mayús+↑ / ↓ | Partida anterior / siguiente sin precio |
| Clic, Mayús+clic, Ctrl+clic | Elegir una línea, un bloque o líneas sueltas del descompuesto |
| Ctrl+C / Ctrl+V | Copiar las líneas elegidas y pegarlas en otra partida (también en otra ventana) |
| Supr | Quitar las líneas elegidas |
| Ctrl+Z / Ctrl+Y | Deshacer / rehacer |
| Ctrl+B | Buscar la partida en las bases |
| F1 | Este manual |

**Varias líneas a la vez**: elige las líneas en el código (clic, Mayús+clic, Ctrl+clic) y con el botón derecho: copiar, **«Copiar a varias partidas…»** (lista con filtro y casillas), **«Guardar como kit…»** o quitar. Si al pegar una línea ya está en la partida, pregunta si sumar la cantidad o dejarla. Cada pegado se deshace de una vez. Las subpartidas se abren con **→**.

**Kits**: grupos de líneas que se repiten (por ejemplo «Toma de fancoil»). Se guardan en la biblioteca compartida con «Guardar como kit…». En el buscador («kit …») salen de dos formas:

- **como líneas sueltas**: se añaden a la partida multiplicadas por la cantidad;
- **como subpartida**: se crea una vez por obra (KIT001…) y se reutiliza.

En la ventana Biblioteca, sección «Kits», se ven sus líneas, precio y último cambio, y se borran.

**Traer partidas de la base**: abre la ventana de bases (Ctrl+B), busca la partida y arrástrala al capítulo o a la partida de la obra: se copian el descompuesto, los recursos y los descuentos.

**Lo que entra en la obra se pone al día con la biblioteca**: lo que viene de una base, de otra obra (Ctrl+V), de un kit o de «Emparejar» toma el PVP de la biblioteca (si el recurso tiene referencia) y, si la obra no tiene descuento para ese fabricante, el de la biblioteca. Lo que ya estaba en la obra no cambia. Sale el aviso «Al día con la biblioteca: …» y se deshace junto con la copia.

## 7. Carpeta de tarifas y biblioteca compartida

**Carpeta de tarifas** (Biblioteca → «Carpeta de tarifas»): los documentos de tarifa de cada fabricante, la única fuente de los PVP.

- Una carpeta por fabricante, con su nombre en mayúsculas igual que en los materiales (KOOLAIR, TROX, S&P…).
- Cada tarifa en PDF o Excel con el nombre empezando por su fecha `AAAA-MM` («2026-05 Tarifa KOOLAIR.pdf»). La más reciente es la vigente; las anteriores quedan como historial.
- «Añadir tarifa…» la copia a su carpeta con el nombre correcto, sin sobrescribir.
- Solo tarifas generales; las ofertas de una obra van con la obra.

**🖹 junto al PVP** de un material del descompuesto: abre la tarifa de su fabricante en la página donde aparece su referencia (con Edge o Chrome). La primera vez lee el PDF entero (unos segundos) y guarda el texto al lado (`… .paginas.json`). Si la referencia no aparece, abre la vigente por el principio y avisa. Las tarifas en Excel se abren, pero no en la fila.

**Biblioteca compartida** (Biblioteca → «Compartir en una carpeta…», por ejemplo en OneDrive): cada persona guarda en su propio fichero y al leer se juntan todos; de cada dato gana el cambio más reciente, con quién y cuándo.

- **Al guardar una obra**: si tiene PVP o descuentos que la biblioteca no tiene o tiene distintos, pregunta si se suben.
- **Al abrir una obra**: si la biblioteca tiene PVP o descuentos distintos (por ejemplo, los que ha cambiado un compañero), avisa con la lista y quién los cambió; la obra cambia solo si se acepta.

**Precios ciertos y estimados**: cada PVP de la biblioteca lleva su origen y su fuente. **Cierto** si sale de una tarifa, de Saltoki Online, de una oferta o de una obra con oferta registrada; **revisado** si viene de una obra o de una base; **estimado** si no tiene fuente. Cuando le pase a Claude precios de Saltoki o de un fabricante, los guarda en la biblioteca (sin borrar el resto de la tarifa), con la fecha y la fuente, para los dos. La comprobación antes de valorar dice cuántos PVP de la biblioteca son ciertos y avisa de los materiales de la obra con PVP estimado.

## 8. Venta: indirectos, gastos generales y beneficio

El coste directo (CD) sale de los descompuestos. La venta se monta en «Resumen y venta»:

1. **Costes indirectos (%)**: gastos de obra que no van en ninguna partida (grúa, permisos, casetas), repartidos como porcentaje sobre todo el CD. PEM = CD + indirectos.
2. **Gastos generales y beneficio industrial (%)**: sobre el PEM.
3. **IVA**: sobre la base de venta.

**Ajustes por partida**: «Sobrecoste de esta partida %» (si no se fija, el de la obra) y «Venta bloqueada» (fija el precio de venta aunque cambie el coste; se ve el margen resultante).

Al cliente solo le llega el precio de venta repercutido: las observaciones y los textos no llevan costes internos.

**Comparar con obras parecidas** (Obra → «Comparar con obras parecidas…»): los ratios de la obra frente a la mediana de las obras del mismo tipo y tamaño parecido guardadas en la biblioteca compartida.

- Garaje: €/plaza, €/m² de garaje y horas de mano de obra por plaza; PCI: €/m² construido y €/vivienda; clima, ventilación de viviendas y fontanería: €/vivienda y €/m² útil. Además, el importe de cada grupo de capítulo (ventilación, detección, BIE…) y el reparto entre materiales, mano de obra y subcontratas.
- Parecidas: mismo tipo y tamaño de la mitad al doble (si hay menos de 3, todas las del tipo, y lo avisa). Si hay 3 o más adjudicadas, solo esas.
- En rojo, lo que se aparta más de un 25 % de la mediana; «Ir» lleva al capítulo.
- Cada obra deja su «huella» al guardarla (si tiene especialidad y datos de la obra). Las contratadas cuentan como adjudicadas; las demás, según el «Estado de la oferta» de Datos de la obra (ofertada, adjudicada, perdida).
- Los importes de otros años no se actualizan: tenlo en cuenta con obras antiguas.

Claude hace la misma comparación (`comparar_obras`) y explica cada desviación antes de dar la oferta por buena.

**Planos DXF**: si la carpeta de documentos tiene planos en DXF, Claude cuenta los aparatos dibujados (bloques por nombre y capa: detectores, BIE, extintores, rejillas…), lee las leyendas y mide la distancia de cada aparato al más cercano y la superficie que cubren, para comprobar la medición del cliente y las coberturas. Los planos en PDF solo aportan sus textos. Los **DWG** se convierten solos a DXF (en la misma carpeta) si está LibreDWG: descargue el paquete de Windows (win64) de https://github.com/LibreDWG/libredwg/releases (libre, GPL-3) y deje `dwg2dxf.exe` y sus DLL junto a `ppto-gui.exe`; si no, guárdelos como DXF desde el CAD.

**Modelos IFC (BIM)**: si hay un `.ifc` (IFC2x3 o IFC4, sin comprimir), Claude cuenta los elementos de instalaciones por clase, tipo, tamaño y planta (difusores, detectores, BIE, unidades, válvulas…), suma los metros de conductos y tuberías cuando el modelo trae la longitud (en sus propiedades o por la extrusión de la geometría) y da los m² de los espacios por planta. Pídale, por ejemplo, «cuenta los elementos del IFC» o «filtra el IFC por detector». Si un modelo no trae longitudes, cuenta las piezas pero no las mide.

### Varios agentes (Claude y ChatGPT)

Si además de Claude conecta Codex (ChatGPT) al programa (ver `docs/mcp.md`), cada propuesta dice quién la hace («Propone: Claude / ChatGPT»). Si los dos proponen cosas distintas para la misma partida, sale **⚖ Desacuerdo** con el precio de cada uno: elija una (la otra queda rechazada); «Aceptar N propuestas» no toca esas partidas. Debajo de cada propuesta salen las opiniones del otro agente (👍 de acuerdo, 👎 en contra, 💬 comentario) con su motivo.

### Certificaciones del cliente (pedido a origen)

Para clientes que mandan su pedido y cada mes una **proforma de certificación a origen** en Excel (Nº línea, descripción, cantidad, precio y «Cantidad a facturar a origen»). Obra → **Certificaciones del cliente (pedido a origen)…**

1. **Importar proforma del cliente…**: crea el pedido (o lo actualiza y dice qué líneas o precios han cambiado) y, si trae cantidades a origen, la certificación de ese mes (el mes sale del nombre del fichero, p. ej. `2026-10_PROFORMA_CERT02.xlsx`). Importe también las de meses anteriores para tener el histórico.
2. **+ Nueva** certificación del mes: empieza con el origen de la anterior; cambie la columna **A origen**. En rojo, lo que pasa del pedido.
3. Lo que se factura es **origen − anterior** línea a línea, **también en negativo** si una línea baja. Avisos: líneas que bajan, excesos sobre el pedido, cambios de precio del cliente y facturas que no cuadran con el origen anterior.
4. **Rellenar proforma del cliente…**: escribe las cantidades a origen en su mismo Excel (solo esa columna).
5. **Detalle de la factura (Excel)…**: para administración, con las líneas del mes, los totales y los avisos.
6. Apunte cada **factura** y cada **abono** (en negativo) en su certificación: el programa avisa si lo facturado no coincide con lo certificado.

### Excesos de medición (certificaciones propias)

En Obra → Seguimiento → Certificaciones, si en una partida se mide más de lo contratado, la partida se queda en el **100 %** y lo que sobra pasa a una partida espejo **EX-código** (mismo precio) en el capítulo **«Excesos de medición»**, al final de la relación valorada. Usted sigue tecleando lo medido en la partida original (sale en naranja); la fila EX se calcula sola.

- **Sin aprobar** (rojo): sale en la certificación, pero **no suma** al total ni se cobra.
- **Aprobado** (verde): pulse el estado de la fila EX, escriba la referencia (correo, acta…) y el motivo y elija «Aprobar». Se cobra **a origen** en la certificación abierta (o en la siguiente si no hay ninguna abierta): todo el exceso acumulado de una vez.
- **Rechazado** (gris): no se cobra; queda como coste propio.
- Un exceso ya cobrado en una certificación cerrada no se puede rechazar sin reabrirla.
- Si el exceso pasa del **10 %** de la partida (cambie el umbral en la pestaña Resumen), sale un aviso ⚠ para pedir la aprobación antes de seguir ejecutando.
- En el PDF los excesos sin aprobar salen en rojo con «[Sin aprobar]»; en el Excel, con su estado, motivo y referencia.

### Legalización (RITE, PCI)

La ventana de legalización calcula la partida de un expediente con el baremo propio: horas por tarea a coste de cada perfil, desplazamientos, y tasas, EICI y visados repercutidos sin margen. Como instalador va a coste y el margen lo pone la obra.

- **Editar el baremo**: horas por tarea, costes por perfil, viajes y tablas. Las memorias tienen su propia visita a obra y su propia jornada (0 = sin redondeo a jornadas).
- **Importar baremo…**: trae un `legalizacion.json` (de un compañero o de Claude). Cada año que trae sustituye al mismo año del baremo propio; los demás años se quedan.

## 9. Trabajar con Claude: propuestas IA

Claude se conecta a la obra abierta en el programa y propone precios, pero nunca cambia nada por su cuenta: todo queda como propuesta hasta que la aceptas.

1. Ten la obra abierta y el ordenador enlazado con Claude (aplicación de escritorio abierta).
2. Pídele lo que necesites: «conéctate al presupuesto», «rellena los datos de la obra con sus documentos», «valora las partidas sin precio con la tarifa X», «propón los contradictorios». Antes de valorar mira los datos y los documentos de la obra (memoria, pliego: marcas, modelos, alcance) y hace la comprobación del apartado 4.
3. Abre **Propuestas IA** en la barra: partida, descompuesto propuesto, precio antes y después, confianza y justificación.
4. **Acepta** o **rechaza** cada una. Al aceptar se aplica y se puede deshacer.

**Tarifas**: Claude busca primero en la carpeta de tarifas y en la biblioteca; las propuestas llegan con PVP, fabricante, grupo y descuento, nunca con netos. Lo que estima sin fuente lo marca aparte.

| Capítulo | Qué va |
| --- | --- |
| 00 OBSERVACIONES | Aclaraciones al cliente sin precio: alcance, lo que hacen terceros, estimaciones repercutidas, límites a verificar. |
| 99 CONTRADICTORIOS | Partidas que faltan en la medición del cliente y se valoran aparte. |

## 10. Exportar: BC3, Presto, PDF y Excel

El BC3 exportado vale exactamente lo mismo que la obra; en Presto hay que pulsar «Calcular todo» tras importarlo.

**BC3 para Presto** (Archivo → Exportar)

- **Descuentos a la vista**: cada material sale a PVP y debajo una línea `CÓDIGO%` «Descuento FABRICANTE/GRUPO x %».
- **Códigos de 12 caracteres** (Presto antiguo): los códigos largos se acortan solos, dejando sitio a la línea `%`.
- **Ajuste de redondeo** (`AJ.REDONDEO`, `AJ.REDONDEO2`…): líneas de pocos céntimos para que cada partida cuadre al céntimo con Presto. Al volver a importar el BC3 se quitan solas.
- **Partidas excluidas e incluidas**: salen con factor 0 en el capítulo.

**En Presto, después de importar**: pulsa **Calcular todo** (hasta entonces las líneas en % salen a 0) y comprueba que el PEM coincide.

**PDF / Imprimir** (Ctrl+P) y **Excel**: presupuesto para el cliente, con «EXCLUIDO» / «INCLUIDO» y sin costes internos.

- La portada muestra el importe **sin IVA** y «A la atención de» bajo el cliente.
- El importe en letra es el **importe sin IVA**: «Asciende el importe total a la expresada cantidad de … (… €), IVA no incluido.»
- **Plantilla propia**: «Guardar copia de la plantilla…» para editarla (Typst) y «Usar plantilla…» para usarla. «Usar la incluida» vuelve a la del programa.

## 11. Buenas prácticas y preguntas frecuentes

**Buenas prácticas**

- Comprueba antes de valorar (apartado 4).
- Siempre PVP y descuento; nada de netos salvo materiales sin tarifa.
- El mismo nombre de fabricante en la tarifa, el descuento y los materiales.
- Antes de estimar un precio, busca en la carpeta de tarifas; si hay que estimar, márcalo aparte.
- Descuentos por fabricante y grupo en la obra, no partida a partida.
- El material primero y el % de accesorios justo debajo de lo que debe afectar.
- Lo que hacen terceros, como partida excluida y nota en 00 OBSERVACIONES.
- Grúa, permisos y gastos de obra, en costes indirectos (%), no en partidas.
- Las horas por dificultad se suben solo en mano de obra propia.
- Guarda una revisión nueva antes de cambios grandes.

**Preguntas frecuentes**

- **En Presto la línea de descuento sale con importe 0.** Falta «Calcular todo».
- **El PEM de Presto no coincide con el del programa.** Exporta de nuevo con una versión actual y pulsa «Calcular todo»; si sigue sin cuadrar, apunta la partida que no cuadra.
- **«No se pudo generar el PDF: no se pudo leer … .typ».** El perfil del PDF usa una plantilla propia que se ha movido o borrado: en la ventana del PDF pulsa «Usar la incluida» o vuelve a elegirla con «Usar plantilla…».
- **Una partida sale como sin precio pero no quiero valorarla.** Márcala como «Excluida» o «Incluida».
- **No encuentro una partida en la base.** Comprueba que la base está añadida y con su especialidad, o usa «Todas a la vez».
- **🖹 no aparece junto a un material.** Ese fabricante no tiene carpeta en la carpeta de tarifas, o el nombre no coincide.
- **Claude no encuentra los documentos de la obra.** Elige la carpeta en Obra → «Datos de la obra…» (📁). Lee PDF, Word, Excel, planos DXF y modelos IFC; los DWG, con LibreDWG (`dwg2dxf.exe` junto a `ppto-gui.exe`) o guardándolos como DXF (Guardar como → DXF).
- **Claude dice que no hay presupuesto abierto.** Abre la obra en el programa y pídele que se conecte otra vez.
