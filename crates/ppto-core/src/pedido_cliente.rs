// SPDX-License-Identifier: GPL-3.0-or-later
//! Certificación sobre el **pedido del cliente**: el cliente (constructora)
//! manda su pedido con sus líneas y precios, y cada mes una proforma «a
//! origen» donde solo se rellena la cantidad a origen de cada línea. Lo que
//! se factura el mes es el origen de este mes menos el del anterior.
//!
//! Aquí está el modelo y las cuentas, con sus comprobaciones: líneas que
//! bajan (van en negativo), excesos sobre el pedido, precios que cambian
//! entre proformas y lo facturado que no cuadra con el origen anterior.

use crate::decimales::redondear;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Pedido de un cliente, con sus certificaciones a origen y sus facturas.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PedidoCliente {
    /// Número de pedido del cliente («EC26_001140»).
    pub numero: String,
    #[serde(default)]
    pub cliente: String,
    #[serde(default)]
    pub fecha: String,
    /// Su número de proyecto u obra, si lo trae.
    #[serde(default)]
    pub proyecto: String,
    pub lineas: Vec<LineaPedido>,
    /// En orden: la 1ª, la 2ª…
    #[serde(default)]
    pub certificaciones: Vec<CertificacionPedido>,
    #[serde(default)]
    pub facturas: Vec<FacturaPedido>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LineaPedido {
    /// Nº de línea del cliente («10000»): es la clave.
    pub linea: String,
    #[serde(default)]
    pub tarea: String,
    pub descripcion: String,
    pub cantidad: Decimal,
    /// Precio con todos los decimales que trae el cliente.
    pub precio: Decimal,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CertificacionPedido {
    pub numero: u32,
    /// Mes que certifica («2026-10»).
    pub mes: String,
    /// Cantidad a origen por nº de línea.
    pub origen: BTreeMap<String, Decimal>,
    /// Precio de cada línea en esta certificación (el pedido puede cambiar).
    pub precios: BTreeMap<String, Decimal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TipoFactura {
    #[default]
    Factura,
    /// Rectificativa: su importe va en negativo.
    Abono,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FacturaPedido {
    pub numero: String,
    pub fecha: String,
    /// Base imponible, con su signo (los abonos en negativo).
    pub importe: Decimal,
    #[serde(default)]
    pub tipo: TipoFactura,
    /// Certificación que factura.
    #[serde(default)]
    pub certificacion: Option<u32>,
    /// Factura que rectifica (abonos).
    #[serde(default)]
    pub rectifica: Option<String>,
}

/// Una línea del detalle del mes.
#[derive(Debug, Clone, PartialEq)]
pub struct LineaMes {
    pub linea: String,
    pub descripcion: String,
    pub cantidad_pedido: Decimal,
    pub origen_anterior: Decimal,
    pub origen: Decimal,
    /// origen − origen anterior (puede ser negativa).
    pub cantidad: Decimal,
    pub precio: Decimal,
    /// Importe a origen de este mes menos el del anterior, a 2 decimales.
    pub importe: Decimal,
    /// El precio cambió respecto a la certificación anterior.
    pub precio_anterior: Option<Decimal>,
}

/// Lo que se factura con una certificación y lo que hay que revisar.
#[derive(Debug, Clone, PartialEq)]
pub struct Mes {
    pub numero: u32,
    pub mes: String,
    pub total_pedido: Decimal,
    /// Total a origen de esta certificación (como lo calcula la proforma).
    pub origen: Decimal,
    pub origen_anterior: Decimal,
    /// origen − origen anterior: lo que hay que facturar.
    pub total: Decimal,
    /// Solo las líneas que cambian este mes.
    pub lineas: Vec<LineaMes>,
    /// Diferencia entre `total` y la suma de las líneas redondeadas.
    pub ajuste_redondeo: Decimal,
    /// Facturado (facturas − abonos) antes de esta certificación.
    pub facturado_antes: Decimal,
    pub avisos: Vec<String>,
}

/// Cambios del pedido al traer una proforma nueva.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CambiosPedido {
    pub nuevas: Vec<String>,
    pub quitadas: Vec<String>,
    /// (línea, precio antes, precio ahora)
    pub precios: Vec<(String, Decimal, Decimal)>,
    /// (línea, cantidad antes, cantidad ahora)
    pub cantidades: Vec<(String, Decimal, Decimal)>,
}

impl CambiosPedido {
    pub fn esta_vacio(&self) -> bool {
        self.nuevas.is_empty() && self.quitadas.is_empty() && self.precios.is_empty() && self.cantidades.is_empty()
    }
}

fn euros(d: Decimal) -> String {
    crate::formato::eur(d)
}

impl PedidoCliente {
    pub fn linea(&self, n: &str) -> Option<&LineaPedido> {
        self.lineas.iter().find(|l| l.linea == n)
    }

    pub fn total(&self) -> Decimal {
        redondear(self.lineas.iter().map(|l| l.cantidad * l.precio).sum(), 2)
    }

    /// Total a origen de una certificación, como la proforma: suma de
    /// cantidad × precio sin redondear y redondeo al final.
    pub fn total_origen(c: &CertificacionPedido) -> Decimal {
        redondear(
            c.origen
                .iter()
                .map(|(l, q)| *q * c.precios.get(l).copied().unwrap_or_default())
                .sum(),
            2,
        )
    }

    /// Actualiza las líneas con las de una proforma nueva y dice qué cambió.
    pub fn actualizar_lineas(&mut self, nuevas: Vec<LineaPedido>) -> CambiosPedido {
        let mut c = CambiosPedido::default();
        for n in &nuevas {
            match self.linea(&n.linea) {
                None => c.nuevas.push(n.linea.clone()),
                Some(v) => {
                    if v.precio != n.precio {
                        c.precios.push((n.linea.clone(), v.precio, n.precio));
                    }
                    if v.cantidad != n.cantidad {
                        c.cantidades.push((n.linea.clone(), v.cantidad, n.cantidad));
                    }
                }
            }
        }
        for v in &self.lineas {
            if !nuevas.iter().any(|n| n.linea == v.linea) {
                c.quitadas.push(v.linea.clone());
            }
        }
        self.lineas = nuevas;
        c
    }

    /// Nueva certificación con el origen de la anterior (o vacía) y los
    /// precios actuales del pedido. Devuelve su número.
    pub fn nueva_certificacion(&mut self, mes: &str) -> u32 {
        let numero = self.certificaciones.last().map_or(1, |c| c.numero + 1);
        let origen = self
            .certificaciones
            .last()
            .map(|c| c.origen.clone())
            .unwrap_or_default();
        let precios = self.lineas.iter().map(|l| (l.linea.clone(), l.precio)).collect();
        self.certificaciones.push(CertificacionPedido {
            numero,
            mes: mes.to_owned(),
            origen,
            precios,
        });
        numero
    }

    /// Pone (o crea) la certificación de un mes con las cantidades a origen
    /// de una proforma, con los precios del pedido en ese momento.
    pub fn fijar_origen(&mut self, mes: &str, origen: BTreeMap<String, Decimal>) -> u32 {
        let precios: BTreeMap<String, Decimal> = self.lineas.iter().map(|l| (l.linea.clone(), l.precio)).collect();
        let origen: BTreeMap<String, Decimal> = origen.into_iter().filter(|(_, q)| !q.is_zero()).collect();
        if let Some(c) = self.certificaciones.iter_mut().find(|c| c.mes == mes) {
            c.origen = origen;
            c.precios = precios;
            return c.numero;
        }
        let numero = self.certificaciones.iter().map(|c| c.numero).max().unwrap_or(0) + 1;
        self.certificaciones.push(CertificacionPedido {
            numero,
            mes: mes.to_owned(),
            origen,
            precios,
        });
        self.certificaciones.sort_by(|a, b| a.mes.cmp(&b.mes));
        for (i, c) in self.certificaciones.iter_mut().enumerate() {
            c.numero = u32::try_from(i + 1).unwrap_or(u32::MAX);
        }
        self.certificaciones
            .iter()
            .find(|c| c.mes == mes)
            .map_or(numero, |c| c.numero)
    }

    /// Facturado (facturas − abonos) de las certificaciones anteriores a
    /// `numero` (las facturas sin certificación cuentan si son de antes).
    fn facturado_antes(&self, numero: u32) -> Decimal {
        self.facturas
            .iter()
            .filter(|f| f.certificacion.is_none_or(|n| n < numero))
            .map(|f| f.importe)
            .sum()
    }

    /// Lo que se factura con la certificación `numero`, con avisos.
    pub fn mes(&self, numero: u32) -> Option<Mes> {
        let i = self.certificaciones.iter().position(|c| c.numero == numero)?;
        let c = &self.certificaciones[i];
        let ant = i.checked_sub(1).map(|j| &self.certificaciones[j]);
        let origen = Self::total_origen(c);
        let origen_anterior = ant.map(Self::total_origen).unwrap_or_default();
        let total = origen - origen_anterior;
        let mut avisos = Vec::new();
        let mut lineas = Vec::new();
        // Cambios de precio de céntimos: se avisan juntos
        let mut centimos = (0usize, Decimal::ZERO);
        // Todas las líneas que salen en esta o en la anterior, en el orden del pedido
        let mut claves: Vec<String> = self.lineas.iter().map(|l| l.linea.clone()).collect();
        for k in c.origen.keys().chain(ant.into_iter().flat_map(|a| a.origen.keys())) {
            if !claves.contains(k) {
                claves.push(k.clone());
            }
        }
        for k in &claves {
            let q = c.origen.get(k).copied().unwrap_or_default();
            let qa = ant.and_then(|a| a.origen.get(k)).copied().unwrap_or_default();
            let p = c.precios.get(k).copied().unwrap_or_default();
            let pa = ant.and_then(|a| a.precios.get(k)).copied();
            let lp = self.linea(k);
            let desc = lp
                .map(|l| l.descripcion.clone())
                .unwrap_or_else(|| format!("(línea {k} ya no está en el pedido)"));
            let cant_pedido = lp.map(|l| l.cantidad).unwrap_or_default();
            if q > cant_pedido && lp.is_some() {
                avisos.push(format!(
                    "Línea {k} «{desc}»: a origen {q} y el pedido es de {cant_pedido}: pide ampliación o contradictorio."
                ));
            }
            if lp.is_none() && !q.is_zero() {
                avisos.push(format!("Línea {k}: tiene origen pero ya no está en el pedido."));
            }
            let importe = redondear(q * p, 2) - redondear(qa * pa.unwrap_or(p), 2);
            if q == qa && importe.is_zero() {
                continue;
            }
            if q < qa {
                avisos.push(format!(
                    "Línea {k} «{desc}»: baja de {qa} a {q} a origen; va en negativo ({}) en la factura del mes.",
                    euros(importe)
                ));
            }
            let cambio_precio = pa.filter(|pa| *pa != p && !qa.is_zero());
            if let Some(pa) = cambio_precio {
                let reg = redondear(qa * (p - pa), 2);
                if reg.abs() >= Decimal::ONE {
                    avisos.push(format!(
                        "Línea {k} «{desc}»: el precio cambia de {pa} a {p}; lo ya certificado se regulariza ({}).",
                        euros(reg)
                    ));
                } else {
                    centimos.0 += 1;
                    centimos.1 += reg;
                }
            }
            lineas.push(LineaMes {
                linea: k.clone(),
                descripcion: desc,
                cantidad_pedido: cant_pedido,
                origen_anterior: qa,
                origen: q,
                cantidad: q - qa,
                precio: p,
                importe,
                precio_anterior: cambio_precio,
            });
        }
        if centimos.0 > 0 {
            avisos.push(format!(
                "{} líneas cambian de precio en céntimos respecto a la certificación anterior (el cliente ha \
                 cambiado los decimales del pedido); lo ya certificado se regulariza en {} en total.",
                centimos.0,
                euros(centimos.1)
            ));
        }
        let suma: Decimal = lineas.iter().map(|l| l.importe).sum();
        let ajuste_redondeo = total - suma;
        let facturado_antes = self.facturado_antes(numero);
        if ant.is_some() && facturado_antes != origen_anterior {
            avisos.push(format!(
                "Lo facturado hasta ahora ({}) no coincide con el origen de la certificación anterior ({}): \
                 diferencia {}. Revise facturas y abonos antes de facturar.",
                euros(facturado_antes),
                euros(origen_anterior),
                euros(facturado_antes - origen_anterior)
            ));
        }
        let de_esta: Vec<&FacturaPedido> = self
            .facturas
            .iter()
            .filter(|f| f.certificacion == Some(numero))
            .collect();
        if !de_esta.is_empty() {
            let f: Decimal = de_esta.iter().map(|f| f.importe).sum();
            if f != total {
                let nums: Vec<&str> = de_esta.iter().map(|f| f.numero.as_str()).collect();
                avisos.push(format!(
                    "Facturado con esta certificación ({}): {} y debería ser {} (diferencia {}).",
                    nums.join(", "),
                    euros(f),
                    euros(total),
                    euros(f - total)
                ));
            }
        }
        Some(Mes {
            numero,
            mes: c.mes.clone(),
            total_pedido: self.total(),
            origen,
            origen_anterior,
            total,
            lineas,
            ajuste_redondeo,
            facturado_antes,
            avisos,
        })
    }

    /// Facturado en total (facturas − abonos).
    pub fn facturado(&self) -> Decimal {
        self.facturas.iter().map(|f| f.importe).sum()
    }

    /// Lo que queda del pedido por certificar.
    pub fn pendiente(&self) -> Decimal {
        self.total() - self.certificaciones.last().map(Self::total_origen).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn linea(n: &str, q: Decimal, p: Decimal) -> LineaPedido {
        LineaPedido {
            linea: n.into(),
            tarea: String::new(),
            descripcion: format!("Línea {n}"),
            cantidad: q,
            precio: p,
        }
    }

    fn origen(v: &[(&str, Decimal)]) -> BTreeMap<String, Decimal> {
        v.iter().map(|(k, q)| ((*k).to_owned(), *q)).collect()
    }

    /// Ejemplo inventado con los mismos casos que una proforma real: una
    /// línea que baja, un precio que cambia y una factura que no cuadra.
    #[test]
    fn mes_con_negativa_cambio_de_precio_y_factura_que_no_cuadra() {
        let mut p = PedidoCliente {
            numero: "PED-1".into(),
            lineas: vec![
                linea("10", dec!(10), dec!(100)),
                linea("20", dec!(50), dec!(20)),
                linea("30", dec!(3), dec!(33.3333)),
            ],
            ..Default::default()
        };
        assert_eq!(p.total(), dec!(2100));
        p.fijar_origen("2026-09", origen(&[("10", dec!(2)), ("20", dec!(30))]));
        // Proforma nueva: el precio de la 30 cambia y aparece la 40
        let cambios = p.actualizar_lineas(vec![
            linea("10", dec!(10), dec!(100)),
            linea("20", dec!(50), dec!(20)),
            linea("30", dec!(3), dec!(33.33333)),
            linea("40", dec!(1), dec!(500)),
        ]);
        assert_eq!(cambios.nuevas, ["40"]);
        assert_eq!(cambios.precios.len(), 1);
        let n = p.fijar_origen("2026-10", origen(&[("10", dec!(5)), ("20", dec!(25)), ("30", dec!(3))]));
        assert_eq!(n, 2);
        p.facturas.push(FacturaPedido {
            numero: "F1".into(),
            fecha: "2026-09-30".into(),
            importe: dec!(800),
            certificacion: Some(1),
            ..Default::default()
        });
        let m = p.mes(2).unwrap();
        assert_eq!(m.origen_anterior, dec!(800));
        // 500 + 500 + 100 = 1100
        assert_eq!(m.origen, dec!(1100));
        assert_eq!(m.total, dec!(300));
        let l20 = m.lineas.iter().find(|l| l.linea == "20").unwrap();
        assert_eq!(l20.cantidad, dec!(-5));
        assert_eq!(l20.importe, dec!(-100));
        assert!(m.avisos.iter().any(|a| a.contains("baja de 30 a 25")));
        assert_eq!(
            m.lineas.iter().map(|l| l.importe).sum::<Decimal>() + m.ajuste_redondeo,
            m.total
        );
        // Factura del mes que no descuenta la negativa
        p.facturas.push(FacturaPedido {
            numero: "F2".into(),
            fecha: "2026-10-31".into(),
            importe: dec!(400),
            certificacion: Some(2),
            ..Default::default()
        });
        let m = p.mes(2).unwrap();
        assert!(
            m.avisos
                .iter()
                .any(|a| a.contains("debería ser") && a.contains("100,00"))
        );
        // Abono de la primera: lo facturado antes ya no cuadra
        p.facturas.push(FacturaPedido {
            numero: "A1".into(),
            fecha: "2026-10-01".into(),
            importe: dec!(-800),
            tipo: TipoFactura::Abono,
            certificacion: Some(1),
            rectifica: Some("F1".into()),
        });
        assert!(p.mes(2).unwrap().avisos.iter().any(|a| a.contains("no coincide")));
        assert_eq!(p.pendiente(), dec!(1500));
    }

    #[test]
    fn exceso_sobre_el_pedido_y_nueva_certificacion() {
        let mut p = PedidoCliente {
            lineas: vec![linea("10", dec!(2), dec!(10))],
            ..Default::default()
        };
        p.fijar_origen("2026-09", origen(&[("10", dec!(1))]));
        let n = p.nueva_certificacion("2026-10");
        assert_eq!(n, 2);
        p.certificaciones[1].origen.insert("10".into(), dec!(3));
        let m = p.mes(2).unwrap();
        assert!(m.avisos.iter().any(|a| a.contains("ampliación")));
        assert_eq!(m.total, dec!(20));
    }
}
