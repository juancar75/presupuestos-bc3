# Servidor MCP (P-020)

`ppto-mcp` permite que Claude (Claude Desktop u otro cliente MCP) consulte y
simule presupuestos. Protocolo JSON-RPC 2.0 por entrada/salida estándar,
implementado sin dependencias externas salvo `serde_json`.

## Instalación en Windows

```powershell
cd $HOME\Documents\presupuestos-bc3
cargo build --release -p ppto-mcp
```

Añadir en `%APPDATA%\Claude\claude_desktop_config.json`, dentro de
`"mcpServers"` (junto a los que ya haya):

```json
"presupuestos": {
  "command": "C:\\Users\\<usuario>\\Documents\\presupuestos-bc3\\target\\release\\ppto-mcp.exe"
}
```

Reiniciar Claude Desktop. Opcionalmente, `"args": ["C:\\ruta\\obra.bc3"]` abre
ese presupuesto al arrancar.

## Herramientas

| Herramienta | Qué hace | ¿Modifica algo? |
|---|---|---|
| `abrir_presupuesto` | Abre un `.bc3` (con informe de importación) o un `.sqlite` | No |
| `abrir_ejemplo` | Abre el ejemplo sintético de suelo radiante | No |
| `resumen` | Capítulos, coste directo, indirectos, PEM, GG, BI, IVA, total | No |
| `buscar` | Busca por código, resumen o texto (sin tildes ni mayúsculas) | No |
| `ver_concepto` | Precio, descompuesto, capítulos donde aparece, mediciones y texto | No |
| `recursos` | Recursos necesarios y horas por oficio | No |
| `simular_cambios` | CI, precios de recursos y cantidades: PEM antes/después y partidas afectadas | No (trabaja sobre una copia) |
| `simular_subcontrata` | Subcontratar parte de una partida por unidad (con factor) o alzado | No |
| `exportar_excel` | Crea el informe Excel en un fichero nuevo | Crea un fichero |
| `exportar_bc3` | Crea un BC3 en un fichero nuevo | Crea un fichero |

Las exportaciones exigen la extensión correcta y **se niegan a sobrescribir el
fichero de origen**. Las modificaciones del presupuesto con aprobación del
usuario y registro en auditoría son la tarea P-021.

## Ejemplos de preguntas

- «Abre C:\…\obra.bc3 y dime qué partidas no cuadran con Presto.»
- «¿Cuántas horas de oficial hay en la obra?»
- «¿Cuánto sube el PEM si el tubo pasa a 1,25 €/m?»
- «Simula subcontratar el montaje del suelo radiante a 1,60 €/m de tubo (7 m por m²).»

## Pruebas

`crates/ppto-mcp/tests/mcp.rs`: protocolo (inicialización, notificaciones,
listado, errores), cada herramienta con cifras calculadas a mano, que las
simulaciones no alteran el presupuesto, protección del fichero de origen y una
sesión real con el binario por stdin/stdout.
