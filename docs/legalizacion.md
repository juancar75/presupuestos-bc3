# Legalización

La ventana **Legalización…** (legalización as-built: proyectos y memorias sobre la instalación ejecutada) añade al presupuesto de la instaladora una
partida por disciplina (`LEG-RITE`, `LEG-PCI`) en el capítulo `LEG`, con
**todos los costes en el descompuesto**. Volver a crearla la sustituye (y se
puede deshacer).

La **tarifa sale de nuestros costes y nuestro margen**:

| Bloque del descompuesto | Cuándo | Cálculo |
|---|---|---|
| Elaboración y tramitación de la memoria técnica | memorias | horas fijas + por 10.000 € de PEM + por vivienda, repartidas por perfil a su coste |
| Tramitación y dirección de la instalación | proyectos | igual, con las horas y el reparto de proyectos; + % de horas si el proyecto es de otro técnico |
| Redacción del proyecto | proyectos propios | horas fijas + por 10.000 € de PEM (+ por vivienda) |
| Justificación RITE | memorias RITE y proyectos de otro técnico | horas fijas + por 10.000 € de PEM |
| Justificación RSIF | si hay circuitos | horas por modelo + horas por circuito repetido |
| Visitas a obra / de la OCA | según nº de visitas | horas del técnico por visita |
| Desplazamientos | por viaje | horas de viaje, km, aparcamiento y, por encima de un umbral, dietas |
| Costes indirectos, gestión y beneficio | siempre | % según el tamaño del encargo (pequeño, mediano, grande por horas, o elegido) |
| Tasas, inspección y visados | si se repercuten | tasa de la Comunidad, EICI, visados (proyectos), EICI de segundas visitas y registro frigorífico; sin margen |

El total de horas se redondea al alza a jornadas completas (8 h por defecto); la diferencia se añade al primer bloque como «ajuste a jornada completa».

Los honorarios en **% del PEM** (tramos y mínimos) no forman el precio: se
calculan como **referencia de mercado** y la ventana muestra la diferencia.

En RITE la ventana aplica el art. 15: con la suma de potencias de los
generadores de calor o, por separado, de frío por encima de 70 kW pasa a
proyecto; de 5 a 70 kW basta memoria; por debajo de 5 kW no hace falta.

También avisa en PCI (Orden 12/03/2014: proyecto si se alcanza alguna
condición de la tabla 1.1 del DB SI 4, como BIE o detección) y en la
instalación frigorífica (RSIF art. 20.2: nivel 2 —sistema > 30 kW, total
> 100 kW, atmósfera artificial o L2/L3— exige proyecto).

**Rellenar desde el presupuesto** lee las mediciones y propone los datos,
cada uno con la partida de la que sale (en naranja, los que conviene
revisar):

| Dato | De dónde sale |
|---|---|
| Base del PEM | capítulos de primer nivel de la disciplina (climatización, incendios…) |
| Potencia de calor y frío | kW del texto de unidades exteriores, bombas de calor, enfriadoras, calderas, roof-top y splits, por su medición (no las unidades interiores) |
| Circuitos RSIF y modelos | unidades exteriores y enfriadoras, y partidas distintas |
| Refrigerante | el más exigente nombrado (R-32/R-454B → A2L; R-410A, R-744 → L1; R-717 → L2; R-290 → L3) |
| Instalación frigorífica | centrales, unidades condensadoras, cámaras y murales; kW eléctricos de compresores si vienen |
| Sistemas PCI, BIE y detección | extintores, BIE, detección, alarma, hidrantes, columna seca, rociadores, abastecimiento, extinción automática, control de humos y señalización |

No se leen el uso del DB SI 4, la superficie, las viviendas, las revisiones
de arquitectura, las visitas ni los km.

## Baremo

`legalizacion.json` en la carpeta de configuración
(`%APPDATA%\presupuestos-bc3`) guarda una versión por año
(`{"versiones": [{"anio": 2026, …}]}`); se usa la del año elegido o la
última anterior. Honorarios y costes se editan en la ventana («Editar
honorarios»); los repartos de horas y las tablas oficiales (tasas, EICI,
visado), en el fichero («Abrir carpeta»). Los campos que faltan valen cero.

El repositorio **no** contiene tarifas reales: `Baremo::ejemplo` lleva
valores sintéticos y es la plantilla que crea la ventana si no hay fichero.
Todos los importes van sin IVA.
