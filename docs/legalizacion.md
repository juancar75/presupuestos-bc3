# Legalización

La ventana **Legalización…** añade al presupuesto de la instaladora los
costes de legalizar una disciplina (RITE o PCI) como partidas con su
descompuesto, dentro del capítulo `LEG` → `LEG-RITE` / `LEG-PCI`. Volver a
crearlo sustituye el subcapítulo (y se puede deshacer).

| Partida | Cuándo | Cálculo |
|---|---|---|
| `TRAM` Coste de tramitación | siempre | % del PEM de la disciplina por tramos acumulativos; se cobra el mayor entre eso, el mínimo por expediente (uno para memorias y otro para proyectos) y, con uso Viviendas, el mínimo por vivienda × nº de viviendas; + recargo si el proyecto es de otro técnico |
| `RED` Redacción del proyecto | proyectos propios | % del PEM por tramos acumulativos con mínimo |
| `OCA` Visita de la OCA | proyectos | importe fijo |
| `VIS2` Segundas visitas | si las hay | n × honorario + n × tarifa EICI (repercutida) |
| `JRITE` Justificación RITE aparte | RITE, opcional (proyectos de otro técnico) | % del PEM con mínimo; en memorias y proyectos propios va incluida |
| `RSIF` Justificación RSIF | si hay circuitos | por circuito de cada modelo distinto + por circuito repetido, con mínimo; registro frigorífico (EICI + tasa) opcional |
| `DESP` Desplazamiento | km por viaje > umbral | km × €/km + horas de viaje × €/h, por viaje |
| `REP` Repercutidos | si se repercuten | tasa de la Comunidad, EICI (memoria, potencia, bloque de viviendas, nº de instalaciones PCI y superficie) y visado del proyecto y de la dirección de obra (solo proyectos) |

La tramitación incluye as-built, cálculos, libro de uso y mantenimiento,
documentación 3.1.5, CIE del instalador eléctrico, dirección de obra y
gestión del expediente.

## Baremo

`legalizacion.json` en la carpeta de configuración
(`%APPDATA%\presupuestos-bc3`) guarda una versión por año
(`{"versiones": [{"anio": 2026, …}]}`); se usa la del año elegido o la
última anterior. Los honorarios propios se editan en la ventana («Editar
honorarios»); las tablas oficiales (tasas, EICI, visado) en el fichero
(«Abrir carpeta»). Los campos que faltan valen cero.

El repositorio **no** contiene tarifas reales: `Baremo::ejemplo` lleva
valores sintéticos y es la plantilla que crea la ventana si no hay fichero.
Todos los importes van sin IVA.
