// SPDX-License-Identifier: GPL-3.0-or-later
//! Informes para comprar y controlar la obra:
//!
//! - [`pedidos`]: lo que hay que pedir a cada **fabricante** (material) y a
//!   cada **subcontrata**, con cantidades totales, PVP, descuento y neto.
//! - [`mano_obra`]: horas previstas por oficio, en total y por capítulo.
//! - [`coste_obra`]: coste por naturaleza y por capítulo frente a la venta.
//!
//! El proveedor de cada recurso sale de su tarifa de fabricante o, si no la
//! tiene, de la oferta que justifica su precio ([`crate::OrigenPrecio`]).

use crate::concepto::Naturaleza;
use crate::error::ErrorMotor;
use crate::explosion::Explosion;
use crate::presupuesto::Presupuesto;
use crate::{Decimal, redondear, venta};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TipoPedido {
    /// Material, maquinaria y otros: se piden a un fabricante o almacén.
    Material,
    /// Trabajos subcontratados.
    Subcontrata,
}

impl TipoPedido {
    pub fn texto(self) -> &'static str {
        match self {
            Self::Material => "Material",
            Self::Subcontrata => "Subcontrata",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineaPedido {
    pub codigo: String,
    /// Referencia del artículo en la oferta, si se conoce.
    pub referencia: Option<String>,
    pub resumen: String,
    pub unidad: String,
    pub cantidad: Decimal,
    /// PVP de tarifa y descuento aplicado (solo si va ligado a tarifa).
    pub pvp: Option<Decimal>,
    pub descuento: Option<Decimal>,
    /// Precio neto unitario.
    pub precio: Decimal,
    pub importe: Decimal,
    /// Oferta de la que sale el precio («nº (fecha)»).
    pub oferta: Option<String>,
    /// Partidas que lo usan.
    pub partidas: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pedido {
    /// Fabricante o subcontrata ("" = sin asignar).
    pub proveedor: String,
    pub tipo: TipoPedido,
    pub lineas: Vec<LineaPedido>,
    pub total: Decimal,
}

impl Pedido {
    pub fn nombre(&self) -> &str {
        if self.proveedor.is_empty() {
            match self.tipo {
                TipoPedido::Material => "(material sin proveedor)",
                TipoPedido::Subcontrata => "(subcontrata sin asignar)",
            }
        } else {
            &self.proveedor
        }
    }
}

/// Proveedor de un recurso: su tarifa o la oferta de su precio.
pub fn proveedor(p: &Presupuesto, codigo: &str) -> String {
    let Some(c) = p.conceptos.get(codigo) else {
        return String::new();
    };
    c.tarifa
        .as_ref()
        .map(|t| t.fabricante.clone())
        .or_else(|| c.origen.as_ref().map(|o| o.fabricante.trim().to_uppercase()))
        .filter(|f| !f.is_empty())
        .unwrap_or_default()
}

/// Partidas que usan cada recurso (directamente o a través de auxiliares).
fn usos(p: &Presupuesto) -> Result<BTreeMap<String, BTreeSet<String>>, ErrorMotor> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for partida in p.cantidades_partidas()?.into_keys() {
        let e = p.explosion_de(&BTreeMap::from([(partida.clone(), Decimal::ONE)]))?;
        for r in e.recursos {
            out.entry(r.codigo).or_default().insert(partida.clone());
        }
    }
    Ok(out)
}

/// Pedidos de la obra por proveedor: primero los fabricantes, después las
/// subcontratas; lo que no tiene proveedor, al final de cada grupo.
pub fn pedidos(p: &Presupuesto) -> Result<Vec<Pedido>, ErrorMotor> {
    let e = p.explosion_recursos()?;
    let usos = usos(p)?;
    let mut m: BTreeMap<(TipoPedido, bool, String), Vec<LineaPedido>> = BTreeMap::new();
    for r in &e.recursos {
        let tipo = match r.naturaleza {
            Naturaleza::Material | Naturaleza::Maquinaria | Naturaleza::Otros => TipoPedido::Material,
            Naturaleza::Subcontrata => TipoPedido::Subcontrata,
            _ => continue,
        };
        if r.cantidad.is_zero() {
            continue;
        }
        let c = p.concepto(&r.codigo)?;
        let prov = proveedor(p, &r.codigo);
        let origen = c.origen.as_ref();
        m.entry((tipo, prov.is_empty(), prov)).or_default().push(LineaPedido {
            codigo: r.codigo.clone(),
            referencia: origen.and_then(|o| o.referencia.clone()),
            resumen: r.resumen.clone(),
            unidad: r.unidad.clone(),
            cantidad: r.cantidad,
            pvp: c.tarifa.as_ref().map(|t| t.pvp),
            descuento: c.tarifa.as_ref().map(|t| p.descuento_tarifa(t)),
            precio: r.precio,
            importe: r.importe,
            oferta: origen.filter(|o| !o.oferta.is_empty()).map(|o| {
                if o.fecha.is_empty() {
                    o.oferta.clone()
                } else {
                    format!("{} ({})", o.oferta, o.fecha)
                }
            }),
            partidas: usos
                .get(&r.codigo)
                .map(|s| s.iter().cloned().collect())
                .unwrap_or_default(),
        });
    }
    Ok(m.into_iter()
        .map(|((tipo, _, proveedor), lineas)| Pedido {
            total: lineas.iter().map(|l| l.importe).sum(),
            proveedor,
            tipo,
            lineas,
        })
        .collect())
}

/// Un oficio (recurso de mano de obra) con sus horas.
#[derive(Debug, Clone, PartialEq)]
pub struct Oficio {
    pub codigo: String,
    pub resumen: String,
    pub unidad: String,
    pub cantidad: Decimal,
    pub precio: Decimal,
    pub importe: Decimal,
}

impl Oficio {
    /// Se mide en horas (las jornadas se calculan a 8 h).
    pub fn en_horas(&self) -> bool {
        es_hora(&self.unidad)
    }
}

pub fn es_hora(unidad: &str) -> bool {
    matches!(unidad.trim().to_lowercase().as_str(), "h" | "hr" | "hora" | "horas")
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManoObraCapitulo {
    pub capitulo: String,
    pub resumen: String,
    pub oficios: Vec<Oficio>,
    pub horas: Decimal,
    pub importe: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManoObra {
    pub oficios: Vec<Oficio>,
    pub capitulos: Vec<ManoObraCapitulo>,
    pub horas: Decimal,
    pub importe: Decimal,
}

fn oficios(e: &Explosion) -> (Vec<Oficio>, Decimal, Decimal) {
    let v: Vec<Oficio> = e
        .por_naturaleza(Naturaleza::ManoObra)
        .filter(|r| !r.cantidad.is_zero())
        .map(|r| Oficio {
            codigo: r.codigo.clone(),
            resumen: r.resumen.clone(),
            unidad: r.unidad.clone(),
            cantidad: r.cantidad,
            precio: r.precio,
            importe: r.importe,
        })
        .collect();
    let horas = v.iter().filter(|o| o.en_horas()).map(|o| o.cantidad).sum();
    let importe = v.iter().map(|o| o.importe).sum();
    (v, horas, importe)
}

/// Capítulos de primer nivel ("" agrupa las partidas sueltas de la raíz).
fn de_capitulos(p: &Presupuesto) -> Result<Vec<(String, String, Explosion)>, ErrorMotor> {
    let raiz = p.concepto(&p.raiz)?;
    let mut out = Vec::new();
    let mut sueltas = BTreeMap::new();
    let cant = p.cantidades_partidas()?;
    for l in &raiz.descomposicion {
        let c = p.concepto(&l.hijo)?;
        if c.naturaleza == Naturaleza::Capitulo {
            out.push((c.codigo.clone(), c.resumen.clone(), p.explosion_capitulo(&c.codigo)?));
        } else if let Some(q) = cant.get(&l.hijo) {
            sueltas.insert(l.hijo.clone(), *q);
        }
    }
    if !sueltas.is_empty() {
        out.push((
            String::new(),
            "(partidas sin capítulo)".into(),
            p.explosion_de(&sueltas)?,
        ));
    }
    Ok(out)
}

/// Mano de obra prevista: horas por oficio en toda la obra y por capítulo.
pub fn mano_obra(p: &Presupuesto) -> Result<ManoObra, ErrorMotor> {
    let (oficios_obra, horas, importe) = oficios(&p.explosion_recursos()?);
    let capitulos = de_capitulos(p)?
        .into_iter()
        .filter_map(|(capitulo, resumen, e)| {
            let (oficios, horas, importe) = oficios(&e);
            (!oficios.is_empty()).then_some(ManoObraCapitulo {
                capitulo,
                resumen,
                oficios,
                horas,
                importe,
            })
        })
        .collect();
    Ok(ManoObra {
        oficios: oficios_obra,
        capitulos,
        horas,
        importe,
    })
}

/// Coste por naturaleza.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Desglose {
    pub material: Decimal,
    pub mano_obra: Decimal,
    pub maquinaria: Decimal,
    pub subcontrata: Decimal,
    /// Otros y medios auxiliares (%).
    pub otros: Decimal,
}

impl Desglose {
    fn de(e: &Explosion) -> Self {
        let mut d = Self::default();
        for r in &e.recursos {
            let x = match r.naturaleza {
                Naturaleza::Material => &mut d.material,
                Naturaleza::ManoObra => &mut d.mano_obra,
                Naturaleza::Maquinaria => &mut d.maquinaria,
                Naturaleza::Subcontrata => &mut d.subcontrata,
                _ => &mut d.otros,
            };
            *x += r.importe;
        }
        d
    }

    pub fn total(&self) -> Decimal {
        self.material + self.mano_obra + self.maquinaria + self.subcontrata + self.otros
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CosteCapitulo {
    pub capitulo: String,
    pub resumen: String,
    pub desglose: Desglose,
    /// Coste con costes indirectos (importe del capítulo en el PEM).
    pub coste: Decimal,
    /// Venta sin IVA (margen de cada partida o GG + BI de la obra).
    pub venta: Decimal,
}

impl CosteCapitulo {
    pub fn beneficio(&self) -> Decimal {
        self.venta - self.coste
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CosteObra {
    pub desglose: Desglose,
    pub coste_directo: Decimal,
    pub costes_indirectos: Decimal,
    /// Coste total (PEM).
    pub pem: Decimal,
    pub venta: Decimal,
    pub capitulos: Vec<CosteCapitulo>,
}

impl CosteObra {
    pub fn beneficio(&self) -> Decimal {
        self.venta - self.pem
    }

    /// Beneficio en % sobre la venta.
    pub fn margen_pct(&self) -> Option<Decimal> {
        (!self.venta.is_zero()).then(|| (self.beneficio() / self.venta * Decimal::ONE_HUNDRED).round_dp(2))
    }
}

/// Coste de la obra frente a la venta, con `gg` y `bi` (%) de la obra para
/// las partidas sin margen propio.
pub fn coste_obra(p: &Presupuesto, gg: Decimal, bi: Decimal) -> Result<CosteObra, ErrorMotor> {
    let v = p.valorar()?;
    let d = p.decimales.importe;
    // Venta de un capítulo: suma de sus partidas a precio de venta
    fn venta_rec(
        p: &Presupuesto,
        v: &crate::Valoracion,
        cap: &str,
        mult: Decimal,
        gg: Decimal,
        bi: Decimal,
        d: u32,
    ) -> Decimal {
        let mut t = Decimal::ZERO;
        for l in v.lineas.get(cap).into_iter().flatten() {
            let Some(c) = p.conceptos.get(&l.hijo) else { continue };
            if c.naturaleza == Naturaleza::Capitulo {
                t += venta_rec(p, v, &l.hijo, mult * l.cantidad, gg, bi, d);
            } else {
                let pv = venta::venta_concepto(l.precio, c, gg, bi, d).venta;
                t += redondear(pv * l.cantidad * mult, d);
            }
        }
        t
    }
    let e = p.explosion_recursos()?;
    let raiz_lineas = v.lineas.get(&p.raiz).cloned().unwrap_or_default();
    let mut capitulos = Vec::new();
    for (codigo, resumen, ex) in de_capitulos(p)? {
        let (coste, venta) = if codigo.is_empty() {
            // Partidas sueltas en la raíz
            let mut c = Decimal::ZERO;
            let mut vt = Decimal::ZERO;
            for l in &raiz_lineas {
                let Some(con) = p.conceptos.get(&l.hijo) else { continue };
                if con.naturaleza != Naturaleza::Capitulo {
                    c += l.importe;
                    vt += redondear(venta::venta_concepto(l.precio, con, gg, bi, d).venta * l.cantidad, d);
                }
            }
            (c, vt)
        } else {
            let l = raiz_lineas.iter().find(|l| l.hijo == codigo);
            let mult = l.map_or(Decimal::ONE, |l| l.cantidad);
            (
                l.map_or(Decimal::ZERO, |l| l.importe),
                venta_rec(p, &v, &codigo, mult, gg, bi, d),
            )
        };
        capitulos.push(CosteCapitulo {
            capitulo: codigo,
            resumen,
            desglose: Desglose::de(&ex),
            coste,
            venta,
        });
    }
    Ok(CosteObra {
        desglose: Desglose::de(&e),
        coste_directo: v.coste_directo,
        costes_indirectos: v.pem - v.coste_directo,
        pem: v.pem,
        venta: capitulos.iter().map(|c| c.venta).sum(),
        capitulos,
    })
}
