# ADR-0005: Subcontratación como escenario sobre presupuesto inmutable

- Estado: Propuesto
- Fecha: 2026-10-09
- Issues: P-016, P-017, P-018, P-019

## Contexto

En instalaciones es habitual presupuestar con mano de obra propia y después
contratar parte del trabajo (montaje de suelo radiante, aislamiento,
conductos) a un subcontratista que cotiza en otra unidad: m², ml de tubo, ud
o alzado. Hay que comparar ofertas sin perder el presupuesto comercial ni
duplicar costes.

## Decisión

- Los **paquetes de trabajo** referencian recursos concretos de partidas
  concretas, con unidad de contratación y factor de conversión.
- La simulación recibe el presupuesto por referencia inmutable y devuelve un
  resultado con coste retirado, coste contratado, PEM del escenario, ahorro,
  horas liberadas por oficio y precio equivalente por unidad de partida.
- Un mismo recurso de una partida no puede pertenecer a dos paquetes
  (error `DobleSustitucion`).
- Las adjudicaciones se guardarán aparte del presupuesto comercial (P-019).

## Consecuencias

- El presupuesto comercial y el de coste (objetivo) conviven sin copiar.
- Pendiente de revisión técnica: si las líneas porcentuales deben
  recalcularse al retirar mano de obra (hoy no se recalculan y se avisa).
