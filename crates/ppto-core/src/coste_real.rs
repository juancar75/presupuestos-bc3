// SPDX-License-Identifier: GPL-3.0-or-later
//! **Coste real** de la obra frente al previsto.
//!
//! Se apuntan los gastos (facturas de proveedores y subcontratas, horas de
//! mano de obra propia…) con su tipo y, si se sabe, el capítulo. El
//! [`informe`](Presupuesto::coste_real) compara por capítulo y por tipo lo
//! previsto en el presupuesto con lo gastado, y el margen real con lo
//! certificado a origen.

use crate::error::ErrorMotor;
use crate::pedidos::coste_obra;
use crate::presupuesto::Presupuesto;
use crate::seguimiento::{FilaRelacion, Precios};
use crate::{Decimal, redondear};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub enum TipoGasto {
    #[default]
    Material,
    ManoObra,
    Subcontrata,
    /// Maquinaria, medios auxiliares, varios.
    Otros,
}

impl TipoGasto {
    pub const TODOS: [Self; 4] = [Self::Material, Self::ManoObra, Self::Subcontrata, Self::Otros];

    pub fn nombre(self) -> &'static str {
        match self {
            Self::Material => "Material",
            Self::ManoObra => "Mano de obra",
            Self::Subcontrata => "Subcontrata",
            Self::Otros => "Otros",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gasto {
    pub numero: u32,
    pub fecha: String,
    pub tipo: TipoGasto,
    /// Proveedor, subcontrata u operario.
    #[serde(default)]
    pub proveedor: String,
    /// Nº de factura, parte de horas…
    #[serde(default)]
    pub documento: String,
    #[serde(default)]
    pub concepto: String,
    /// Capítulo de primer nivel; `None` = de toda la obra.
    #[serde(default)]
    pub capitulo: Option<String>,
    /// Horas (mano de obra), solo informativo.
    #[serde(default)]
    pub horas: Option<Decimal>,
    /// Sin IVA.
    pub importe: Decimal,
    /// Pedido registrado al que corresponde.
    #[serde(default)]
    pub pedido: Option<u32>,
}

/// Importes por tipo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PorTipo {
    pub material: Decimal,
    pub mano_obra: Decimal,
    pub subcontrata: Decimal,
    pub otros: Decimal,
}

impl PorTipo {
    pub fn total(&self) -> Decimal {
        self.material + self.mano_obra + self.subcontrata + self.otros
    }

    pub fn de(&self, t: TipoGasto) -> Decimal {
        match t {
            TipoGasto::Material => self.material,
            TipoGasto::ManoObra => self.mano_obra,
            TipoGasto::Subcontrata => self.subcontrata,
            TipoGasto::Otros => self.otros,
        }
    }

    fn sumar(&mut self, t: TipoGasto, x: Decimal) {
        *match t {
            TipoGasto::Material => &mut self.material,
            TipoGasto::ManoObra => &mut self.mano_obra,
            TipoGasto::Subcontrata => &mut self.subcontrata,
            TipoGasto::Otros => &mut self.otros,
        } += x;
    }

    fn mas(&mut self, o: &PorTipo) {
        for t in TipoGasto::TODOS {
            self.sumar(t, o.de(t));
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FilaCosteReal {
    /// Vacío: gastos sin capítulo (o partidas sueltas en la raíz).
    pub capitulo: String,
    pub resumen: String,
    /// Coste directo previsto.
    pub previsto: PorTipo,
    pub real: PorTipo,
    /// Venta del presupuesto (sin IVA).
    pub venta: Decimal,
    /// Certificado a origen, a precio de venta.
    pub certificado: Decimal,
}

impl FilaCosteReal {
    /// % del coste previsto ya gastado.
    pub fn gastado_pct(&self) -> Option<Decimal> {
        let p = self.previsto.total();
        (!p.is_zero()).then(|| redondear(self.real.total() / p * Decimal::ONE_HUNDRED, 1))
    }

    /// % de la venta ya certificado.
    pub fn certificado_pct(&self) -> Option<Decimal> {
        (!self.venta.is_zero()).then(|| redondear(self.certificado / self.venta * Decimal::ONE_HUNDRED, 1))
    }

    /// Margen real a fecha: certificado − gastado.
    pub fn margen_real(&self) -> Decimal {
        self.certificado - self.real.total()
    }

    /// Margen previsto: venta − coste previsto.
    pub fn margen_previsto(&self) -> Decimal {
        self.venta - self.previsto.total()
    }

    /// El gasto va por delante de lo certificado (más % gastado que
    /// certificado, con un 5 % de margen).
    pub fn alerta(&self) -> bool {
        match (self.gastado_pct(), self.certificado_pct()) {
            (Some(g), Some(c)) => g > c + Decimal::from(5),
            (None, _) => !self.real.total().is_zero(),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CosteReal {
    pub capitulos: Vec<FilaCosteReal>,
    pub total: FilaCosteReal,
    /// Costes indirectos previstos (no se reparten por tipo).
    pub indirectos_previstos: Decimal,
    pub horas_reales: Decimal,
}

fn error(m: impl Into<String>) -> ErrorMotor {
    ErrorMotor::ValorInvalido(m.into())
}

impl Presupuesto {
    /// Apunta un gasto (se le da número). Devuelve el número.
    pub fn anotar_gasto(&mut self, mut g: Gasto) -> Result<u32, ErrorMotor> {
        if let Some(c) = &g.capitulo
            && !self.conceptos.contains_key(c)
        {
            return Err(error(format!("el capítulo {c} no existe")));
        }
        g.numero = self.seguimiento.gastos.iter().map(|g| g.numero).max().unwrap_or(0) + 1;
        let n = g.numero;
        self.seguimiento.gastos.push(g);
        Ok(n)
    }

    /// Sustituye un gasto (mismo número).
    pub fn cambiar_gasto(&mut self, g: Gasto) -> Result<(), ErrorMotor> {
        let x = self
            .seguimiento
            .gastos
            .iter_mut()
            .find(|x| x.numero == g.numero)
            .ok_or_else(|| error(format!("no existe el gasto nº {}", g.numero)))?;
        *x = g;
        Ok(())
    }

    pub fn borrar_gasto(&mut self, numero: u32) {
        self.seguimiento.gastos.retain(|g| g.numero != numero);
    }

    /// Coste real frente al previsto, por capítulo de primer nivel.
    pub fn coste_real(&self, gg: Decimal, bi: Decimal) -> Result<CosteReal, ErrorMotor> {
        let c = coste_obra(self, gg, bi)?;
        // Certificado a origen por capítulo de primer nivel
        let mut cert: BTreeMap<String, Decimal> = BTreeMap::new();
        if let Some(u) = self.seguimiento.certificaciones.last() {
            let r = self.relacion_valorada(u.numero, Precios::Venta { gg, bi })?;
            let mut sueltas = Decimal::ZERO;
            for f in &r.filas {
                match f {
                    FilaRelacion::Capitulo {
                        codigo,
                        nivel: 0,
                        importes,
                        ..
                    } => {
                        cert.insert(codigo.clone(), importes.origen);
                    }
                    FilaRelacion::Partida(x) if x.capitulo == self.raiz => sueltas += x.importes.origen,
                    _ => {}
                }
            }
            if !sueltas.is_zero() {
                *cert.entry(String::new()).or_default() += sueltas;
            }
        }
        let mut filas: Vec<FilaCosteReal> = c
            .capitulos
            .iter()
            .map(|x| {
                let d = &x.desglose;
                FilaCosteReal {
                    capitulo: x.capitulo.clone(),
                    resumen: x.resumen.clone(),
                    previsto: PorTipo {
                        material: d.material,
                        mano_obra: d.mano_obra,
                        subcontrata: d.subcontrata,
                        otros: d.maquinaria + d.otros + d.sin_descomponer,
                    },
                    real: PorTipo::default(),
                    venta: x.venta,
                    certificado: cert.get(&x.capitulo).copied().unwrap_or_default(),
                }
            })
            .collect();
        let mut horas = Decimal::ZERO;
        for g in &self.seguimiento.gastos {
            horas += g.horas.unwrap_or_default();
            let codigo = g.capitulo.clone().unwrap_or_default();
            let i = match filas.iter().position(|f| f.capitulo == codigo) {
                Some(i) => i,
                None => {
                    filas.push(FilaCosteReal {
                        capitulo: codigo.clone(),
                        resumen: if codigo.is_empty() {
                            "Gastos generales de la obra (sin capítulo)".into()
                        } else {
                            self.conceptos
                                .get(&codigo)
                                .map(|c| c.resumen.clone())
                                .unwrap_or_default()
                        },
                        previsto: PorTipo::default(),
                        real: PorTipo::default(),
                        venta: Decimal::ZERO,
                        certificado: Decimal::ZERO,
                    });
                    filas.len() - 1
                }
            };
            filas[i].real.sumar(g.tipo, g.importe);
        }
        let mut total = FilaCosteReal {
            capitulo: String::new(),
            resumen: "Total obra".into(),
            previsto: PorTipo::default(),
            real: PorTipo::default(),
            venta: Decimal::ZERO,
            certificado: Decimal::ZERO,
        };
        for f in &filas {
            total.previsto.mas(&f.previsto);
            total.real.mas(&f.real);
            total.venta += f.venta;
            total.certificado += f.certificado;
        }
        Ok(CosteReal {
            capitulos: filas,
            total,
            indirectos_previstos: c.costes_indirectos,
            horas_reales: horas,
        })
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::ejemplos::suelo_radiante;
    use rust_decimal_macros::dec;

    fn gasto(tipo: TipoGasto, capitulo: Option<&str>, importe: Decimal) -> Gasto {
        Gasto {
            numero: 0,
            fecha: "01/11/2026".into(),
            tipo,
            proveedor: "Proveedor".into(),
            documento: "F-1".into(),
            concepto: "Material".into(),
            capitulo: capitulo.map(str::to_owned),
            horas: None,
            importe,
            pedido: None,
        }
    }

    #[test]
    fn previsto_frente_a_real() {
        let mut p = suelo_radiante();
        let c0 = p.coste_real(dec!(13), dec!(6)).unwrap();
        assert_eq!(c0.total.real.total(), Decimal::ZERO);
        assert!(c0.total.previsto.total() > Decimal::ZERO);
        assert!(!c0.total.alerta());
        assert_eq!(
            p.anotar_gasto(gasto(TipoGasto::Material, Some("C01"), dec!(1000)))
                .unwrap(),
            1
        );
        let mut h = gasto(TipoGasto::ManoObra, Some("C01"), dec!(300));
        h.horas = Some(dec!(12));
        p.anotar_gasto(h).unwrap();
        p.anotar_gasto(gasto(TipoGasto::Otros, None, dec!(50))).unwrap();
        assert!(p.anotar_gasto(gasto(TipoGasto::Otros, Some("NO"), dec!(1))).is_err());
        let c = p.coste_real(dec!(13), dec!(6)).unwrap();
        let f = c.capitulos.iter().find(|f| f.capitulo == "C01").unwrap();
        assert_eq!((f.real.material, f.real.mano_obra), (dec!(1000), dec!(300)));
        // Gastado sin certificar nada: alerta
        assert!(f.alerta());
        assert_eq!(f.margen_real(), dec!(-1300));
        let sin = c.capitulos.iter().find(|f| f.capitulo.is_empty()).unwrap();
        assert_eq!(sin.real.otros, dec!(50));
        assert_eq!(c.total.real.total(), dec!(1350));
        assert_eq!(c.horas_reales, dec!(12));
        // Certificar todo: margen real = venta − gastado
        let n = p.nueva_certificacion("1").unwrap();
        for h in ["SR.M2", "SR.COL8"] {
            p.certificar_completa(n, "C01", h).unwrap();
        }
        let c = p.coste_real(dec!(13), dec!(6)).unwrap();
        let f = c.capitulos.iter().find(|f| f.capitulo == "C01").unwrap();
        assert_eq!(f.certificado, f.venta);
        assert_eq!(f.certificado_pct(), Some(dec!(100)));
        assert!(!f.alerta());
        // Cambiar y borrar
        let mut g = p.seguimiento.gastos[0].clone();
        g.importe = dec!(900);
        p.cambiar_gasto(g).unwrap();
        p.borrar_gasto(3);
        let c = p.coste_real(dec!(13), dec!(6)).unwrap();
        assert_eq!(c.total.real.total(), dec!(1200));
        assert!(c.capitulos.iter().all(|f| !f.capitulo.is_empty()));
        let q: Presupuesto = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(q, p);
    }
}
