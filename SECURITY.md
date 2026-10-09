# Política de seguridad

## Versiones con soporte

Mientras el proyecto esté en fase 0.x solo recibe correcciones la última
versión publicada y la rama `main`.

## Cómo informar de una vulnerabilidad

**No abras una Issue pública.** Usa *Security → Report a vulnerability*
(avisos privados de GitHub) en este repositorio. Indica:

- Versión o commit afectado.
- Pasos para reproducirlo con datos **sintéticos**.
- Impacto: alteración de importes, acceso a ficheros, ejecución de código,
  escrituras no autorizadas vía MCP…

Respuesta inicial en un máximo de 7 días.

## Ámbitos especialmente sensibles

- **Importación BC3**: ficheros de terceros; el lector debe resistir entradas
  malformadas o maliciosas sin bloquearse ni agotar memoria.
- **Servidor MCP**: ninguna operación de escritura sin aprobación explícita
  del usuario y registro en auditoría (P-021).
- **Integridad económica**: cualquier vía que altere importes sin dejar
  rastro en la auditoría se trata como vulnerabilidad.
- **Datos de clientes**: si detectas un presupuesto real en el repositorio o
  en sus Issues, avísanos por el canal privado para retirarlo.
