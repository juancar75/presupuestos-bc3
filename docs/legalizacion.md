# Legalización

La ventana **Legalización…** añade al presupuesto de la instaladora una
partida por disciplina (`LEG-RITE`, `LEG-PCI`) en el capítulo `LEG`, con
**todos los costes en el descompuesto**. Volver a crearla la sustituye (y se
puede deshacer).

| Bloque del descompuesto | Cuándo | Cómo se calcula |
|---|---|---|
| Tramitación y dirección de obra | siempre | honorario de baremo: % del PEM por tramos acumulativos; el mayor entre eso, el mínimo por expediente (memoria o proyecto) y, con uso Viviendas, mínimo por vivienda × nº; + recargo si el proyecto es de otro técnico |
| Redacción del proyecto | proyectos propios | % del PEM por tramos con mínimo |
| Justificación RITE | opcional (proyectos de otro técnico) | % del PEM con mínimo |
| Justificación RSIF | si hay circuitos | por circuito de cada modelo + por circuito repetido, con mínimo |
| Visitas de la OCA | proyectos y segundas visitas | horas del técnico por visita a su coste |
| Desplazamientos | visitas de obra + de la OCA | horas de viaje, km, aparcamiento y, por encima de un umbral, dietas |
| Costes indirectos, gestión y beneficio | siempre | % sobre todo lo anterior (línea porcentual) |
| Tasas, inspección y visados | si se repercuten | tasa de la Comunidad, EICI, visado del proyecto y de la dirección de obra (proyectos), EICI de segundas visitas y registro frigorífico; sin margen |

Los honorarios de baremo son **precio de venta**: se descuenta el margen y lo
que queda se reparte en horas de cada perfil (senior, junior, técnico,
documentación) a su coste horario, según el reparto de cada tarea. Así la
partida muestra horas y costes reales y su importe coincide con el baremo.

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
