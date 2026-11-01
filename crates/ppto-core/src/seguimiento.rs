// SPDX-License-Identifier: GPL-3.0-or-later
//! Seguimiento de obra para una instaladora: **contradictorios** (precios
//! nuevos o unidades no previstas, que se presentan al cliente para su
//! aprobación) y **certificaciones** mensuales (relación valorada).
//!
//! - Un contradictorio es un capítulo colgado de la raíz (`CTR-01`…) con
//!   número, fecha y estado. Se valora como cualquier otro capítulo.
//! - Una certificación guarda, por cada partida de cada capítulo, la cantidad
//!   ejecutada **a origen**. Lo anterior sale de la certificación previa y lo
//!   del mes es la diferencia. Al cerrarla, los precios quedan congelados y ya
//!   no se puede modificar (salvo reabrir la última).
//!
//! Todo se calcula a precio de ejecución material (con costes indirectos);
//! GG, BI, IVA y retención se aplican en el resumen, como en el presupuesto.

use crate::decimales::redondear;
use crate::error::ErrorMotor;
use crate::presupuesto::{Presupuesto, Valoracion};
use crate::{Concepto, Naturaleza};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Datos de seguimiento que viajan con el presupuesto.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Seguimiento {
    /// Por código del capítulo del contradictorio.
    #[serde(default)]
    pub contradictorios: BTreeMap<String, Contradictorio>,
    /// En orden: la 1ª, la 2ª…
    #[serde(default)]
    pub certificaciones: Vec<Certificacion>,
    /// Pedidos registrados (ver [`crate::compras`]).
    #[serde(default)]
    pub pedidos: Vec<crate::compras::PedidoRegistrado>,
}

impl Seguimiento {
    pub fn esta_vacio(&self) -> bool {
        self.contradictorios.is_empty() && self.certificaciones.is_empty() && self.pedidos.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EstadoContradictorio {
    #[default]
    Propuesto,
    Aprobado,
    Rechazado,
}

impl EstadoContradictorio {
    pub const TODOS: [Self; 3] = [Self::Propuesto, Self::Aprobado, Self::Rechazado];

    pub fn nombre(self) -> &'static str {
        match self {
            Self::Propuesto => "Pendiente",
            Self::Aprobado => "Aprobado",
            Self::Rechazado => "Rechazado",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contradictorio {
    pub numero: u32,
    /// Texto libre (dd/mm/aaaa).
    pub fecha: String,
    pub estado: EstadoContradictorio,
    /// Motivo, quién lo pidió, referencia de la orden…
    #[serde(default)]
    pub nota: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Certificacion {
    pub numero: u32,
    pub fecha: String,
    pub cerrada: bool,
    pub lineas: Vec<LineaCertificada>,
}

/// Cantidad ejecutada a origen de una partida dentro de un capítulo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineaCertificada {
    pub capitulo: String,
    pub partida: String,
    pub origen: Decimal,
    /// Precio de ejecución material congelado al cerrar.
    #[serde(default)]
    pub precio: Option<Decimal>,
    /// Precio de venta (con GG y BI o el margen de la partida) congelado al
    /// cerrar.
    #[serde(default)]
    pub venta: Option<Decimal>,
}

/// A qué precios se valora la relación.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precios {
    /// Precio de ejecución material; GG y BI se suman en el resumen
    /// (ingeniería, obra pública).
    Ejecucion { gg: Decimal, bi: Decimal },
    /// Precio de venta de cada partida, como en la oferta de la instaladora
    /// (GG y BI, o el margen propio, ya van dentro).
    Venta { gg: Decimal, bi: Decimal },
}

impl Precios {
    fn es_venta(self) -> bool {
        matches!(self, Self::Venta { .. })
    }
}

impl Certificacion {
    pub fn origen(&self, capitulo: &str, partida: &str) -> Decimal {
        self.linea(capitulo, partida).map(|l| l.origen).unwrap_or_default()
    }

    fn linea(&self, capitulo: &str, partida: &str) -> Option<&LineaCertificada> {
        self.lineas
            .iter()
            .find(|l| l.capitulo == capitulo && l.partida == partida)
    }
}

/// Totales del contrato y sus modificaciones (a PEM).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResumenContrato {
    /// Presupuesto sin contradictorios.
    pub contratado: Decimal,
    pub aprobados: Decimal,
    pub pendientes: Decimal,
    pub rechazados: Decimal,
    /// Contratado + aprobados.
    pub vigente: Decimal,
}

/// Una fila de la relación valorada, en el orden del árbol.
#[derive(Debug, Clone, PartialEq)]
pub enum FilaRelacion {
    Capitulo {
        codigo: String,
        resumen: String,
        nivel: usize,
        /// Importes del capítulo (suma de sus partidas, PEM).
        importes: Importes,
    },
    Partida(PartidaCertificada),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Importes {
    pub presupuesto: Decimal,
    pub anterior: Decimal,
    pub actual: Decimal,
    pub origen: Decimal,
}

impl Importes {
    fn sumar(&mut self, o: &Importes) {
        self.presupuesto += o.presupuesto;
        self.anterior += o.anterior;
        self.actual += o.actual;
        self.origen += o.origen;
    }

    /// % ejecutado a origen sobre el presupuesto (0 si no hay presupuesto).
    pub fn porcentaje(&self) -> Decimal {
        if self.presupuesto.is_zero() {
            Decimal::ZERO
        } else {
            redondear(self.origen / self.presupuesto * Decimal::ONE_HUNDRED, 2)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartidaCertificada {
    pub capitulo: String,
    pub codigo: String,
    pub unidad: String,
    pub resumen: String,
    pub nivel: usize,
    /// Cantidad del presupuesto.
    pub medicion: Decimal,
    /// Precio aplicado a lo de este mes (congelado si está cerrada).
    pub precio: Decimal,
    pub cant_anterior: Decimal,
    pub cant_actual: Decimal,
    pub cant_origen: Decimal,
    pub importes: Importes,
    /// Se certifica más de lo presupuestado.
    pub exceso: bool,
}

/// Relación valorada de una certificación.
#[derive(Debug, Clone, PartialEq)]
pub struct Relacion {
    pub numero: u32,
    pub fecha: String,
    pub cerrada: bool,
    pub filas: Vec<FilaRelacion>,
    pub total: Importes,
}

impl Relacion {
    pub fn partidas(&self) -> impl Iterator<Item = &PartidaCertificada> {
        self.filas.iter().filter_map(|f| match f {
            FilaRelacion::Partida(p) => Some(p),
            FilaRelacion::Capitulo { .. } => None,
        })
    }

    /// Partidas con más certificado que presupuestado.
    pub fn excesos(&self) -> Vec<&PartidaCertificada> {
        self.partidas().filter(|p| p.exceso).collect()
    }
}

/// Resumen económico de una certificación: PEM, GG, BI, IVA y retención de
/// lo anterior, lo del mes y lo acumulado.
#[derive(Debug, Clone, PartialEq)]
pub struct ResumenCertificacion {
    pub anterior: crate::venta::ResumenPresupuesto,
    pub actual: crate::venta::ResumenPresupuesto,
    pub origen: crate::venta::ResumenPresupuesto,
    /// Retención de garantía sobre la base del mes.
    pub retencion: Decimal,
    /// Total del mes (con IVA) menos la retención.
    pub a_cobrar: Decimal,
}

fn error(m: impl Into<String>) -> ErrorMotor {
    ErrorMotor::ValorInvalido(m.into())
}

impl Presupuesto {
    // ---------------------------------------------------------------- contradictorios

    /// Siguiente código libre `CTR-01`, `CTR-02`…
    pub fn codigo_contradictorio_libre(&self) -> String {
        (1..)
            .map(|n| format!("CTR-{n:02}"))
            .find(|c| !self.conceptos.contains_key(c))
            .unwrap_or_default()
    }

    /// Crea un contradictorio: un capítulo nuevo colgado de la raíz.
    /// Devuelve su código.
    pub fn nuevo_contradictorio(&mut self, resumen: &str, fecha: &str, nota: &str) -> Result<String, ErrorMotor> {
        let codigo = self.codigo_contradictorio_libre();
        let numero = self
            .seguimiento
            .contradictorios
            .values()
            .map(|c| c.numero)
            .max()
            .unwrap_or(0)
            + 1;
        self.insertar(Concepto::capitulo(codigo.clone(), resumen))?;
        let raiz = self.raiz.clone();
        self.colgar(&raiz, &codigo, None)?;
        self.seguimiento.contradictorios.insert(
            codigo.clone(),
            Contradictorio {
                numero,
                fecha: fecha.to_owned(),
                estado: EstadoContradictorio::Propuesto,
                nota: nota.to_owned(),
            },
        );
        Ok(codigo)
    }

    /// Marca un capítulo existente como contradictorio (o le cambia los datos).
    pub fn marcar_contradictorio(&mut self, codigo: &str, c: Option<Contradictorio>) -> Result<(), ErrorMotor> {
        match c {
            None => {
                self.seguimiento.contradictorios.remove(codigo);
            }
            Some(c) => {
                if self.concepto(codigo)?.naturaleza != Naturaleza::Capitulo {
                    return Err(error(format!("{codigo} no es un capítulo")));
                }
                self.seguimiento.contradictorios.insert(codigo.to_owned(), c);
            }
        }
        Ok(())
    }

    pub fn fijar_estado_contradictorio(
        &mut self,
        codigo: &str,
        estado: EstadoContradictorio,
    ) -> Result<(), ErrorMotor> {
        self.seguimiento
            .contradictorios
            .get_mut(codigo)
            .ok_or_else(|| error(format!("{codigo} no es un contradictorio")))?
            .estado = estado;
        Ok(())
    }

    /// Importe (PEM) de un capítulo tal como entra en la obra: precio del
    /// capítulo por su cantidad en cada padre.
    fn importe_en_obra(&self, v: &Valoracion, codigo: &str) -> Decimal {
        v.lineas
            .values()
            .flatten()
            .filter(|l| l.hijo == codigo)
            .map(|l| l.importe)
            .sum()
    }

    /// Contratado, aprobados, pendientes y rechazados (PEM).
    pub fn resumen_contrato(&self) -> Result<ResumenContrato, ErrorMotor> {
        let v = self.valorar()?;
        let mut r = ResumenContrato::default();
        let mut ctr = Decimal::ZERO;
        for (codigo, c) in &self.seguimiento.contradictorios {
            if !self.conceptos.contains_key(codigo) {
                continue;
            }
            let i = self.importe_en_obra(&v, codigo);
            ctr += i;
            match c.estado {
                EstadoContradictorio::Aprobado => r.aprobados += i,
                EstadoContradictorio::Propuesto => r.pendientes += i,
                EstadoContradictorio::Rechazado => r.rechazados += i,
            }
        }
        r.contratado = v.pem - ctr;
        r.vigente = r.contratado + r.aprobados;
        Ok(r)
    }

    /// Copia del presupuesto con solo ese contradictorio bajo la raíz (para
    /// imprimirlo y que lo firme el cliente).
    pub fn solo_contradictorio(&self, codigo: &str) -> Result<Presupuesto, ErrorMotor> {
        if !self.seguimiento.contradictorios.contains_key(codigo) {
            return Err(error(format!("{codigo} no es un contradictorio")));
        }
        let mut p = self.clone();
        let raiz = p.raiz.clone();
        let fuera: Vec<String> = p
            .concepto(&raiz)?
            .descomposicion
            .iter()
            .map(|l| l.hijo.clone())
            .filter(|h| h != codigo)
            .collect();
        if let Some(r) = p.conceptos.get_mut(&raiz) {
            r.descomposicion.retain(|l| l.hijo == codigo);
        }
        for h in fuera {
            p.mediciones.remove(&(raiz.clone(), h));
        }
        p.seguimiento = Seguimiento::default();
        Ok(p)
    }

    // ---------------------------------------------------------------- certificaciones

    fn certificacion_mut(&mut self, numero: u32) -> Result<&mut Certificacion, ErrorMotor> {
        self.seguimiento
            .certificaciones
            .iter_mut()
            .find(|c| c.numero == numero)
            .ok_or_else(|| error(format!("no existe la certificación nº {numero}")))
    }

    pub fn certificacion(&self, numero: u32) -> Option<&Certificacion> {
        self.seguimiento.certificaciones.iter().find(|c| c.numero == numero)
    }

    fn abierta(&mut self, numero: u32) -> Result<&mut Certificacion, ErrorMotor> {
        let c = self.certificacion_mut(numero)?;
        if c.cerrada {
            return Err(error(format!("la certificación nº {numero} está cerrada")));
        }
        Ok(c)
    }

    /// Abre la certificación siguiente partiendo de lo certificado a origen
    /// en la anterior. Solo puede haber una abierta.
    pub fn nueva_certificacion(&mut self, fecha: &str) -> Result<u32, ErrorMotor> {
        let previa = self.seguimiento.certificaciones.last();
        if previa.is_some_and(|c| !c.cerrada) {
            return Err(error("cierre la certificación abierta antes de empezar otra"));
        }
        let numero = previa.map_or(1, |c| c.numero + 1);
        let lineas = previa
            .map(|c| {
                c.lineas
                    .iter()
                    .map(|l| LineaCertificada {
                        precio: None,
                        venta: None,
                        ..l.clone()
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.seguimiento.certificaciones.push(Certificacion {
            numero,
            fecha: fecha.to_owned(),
            cerrada: false,
            lineas,
        });
        Ok(numero)
    }

    /// Cantidad a origen certificada en la certificación anterior a `numero`.
    fn origen_previo(&self, numero: u32, capitulo: &str, partida: &str) -> Decimal {
        self.seguimiento
            .certificaciones
            .iter()
            .rev()
            .find(|c| c.numero < numero)
            .map(|c| c.origen(capitulo, partida))
            .unwrap_or_default()
    }

    /// Fija la cantidad ejecutada a origen.
    pub fn certificar_origen(
        &mut self,
        numero: u32,
        capitulo: &str,
        partida: &str,
        origen: Decimal,
    ) -> Result<(), ErrorMotor> {
        let colgada = self
            .concepto(capitulo)?
            .descomposicion
            .iter()
            .any(|l| l.hijo == partida);
        if !colgada {
            return Err(error(format!("{partida} no está en el capítulo {capitulo}")));
        }
        let origen = redondear(origen, self.decimales.medicion);
        let c = self.abierta(numero)?;
        match c
            .lineas
            .iter_mut()
            .find(|l| l.capitulo == capitulo && l.partida == partida)
        {
            Some(l) => l.origen = origen,
            None if origen.is_zero() => {}
            None => c.lineas.push(LineaCertificada {
                capitulo: capitulo.to_owned(),
                partida: partida.to_owned(),
                origen,
                precio: None,
                venta: None,
            }),
        }
        c.lineas.retain(|l| !l.origen.is_zero());
        Ok(())
    }

    /// Fija la cantidad ejecutada este mes (a origen = anterior + mes).
    pub fn certificar_mes(
        &mut self,
        numero: u32,
        capitulo: &str,
        partida: &str,
        mes: Decimal,
    ) -> Result<(), ErrorMotor> {
        let previo = self.origen_previo(numero, capitulo, partida);
        self.certificar_origen(numero, capitulo, partida, previo + mes)
    }

    /// Certifica el 100 % de lo presupuestado de una partida.
    pub fn certificar_completa(&mut self, numero: u32, capitulo: &str, partida: &str) -> Result<(), ErrorMotor> {
        let cant = self
            .concepto(capitulo)?
            .descomposicion
            .iter()
            .find(|l| l.hijo == partida)
            .map(|l| redondear(l.cantidad(), self.decimales.medicion))
            .unwrap_or_default();
        self.certificar_origen(numero, capitulo, partida, cant)
    }

    /// Cierra la certificación: congela los precios de ejecución y de venta
    /// (con los GG y BI de la obra, en puntos).
    pub fn cerrar_certificacion(&mut self, numero: u32, gg: Decimal, bi: Decimal) -> Result<(), ErrorMotor> {
        let v = self.valorar()?;
        let precios = precios_en_capitulo(&v);
        self.abierta(numero)?;
        let ventas: Vec<(Decimal, Decimal)> = self
            .certificacion(numero)
            .map(|c| c.lineas.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|l| {
                let pem = precios
                    .get(&(l.capitulo.as_str(), l.partida.as_str()))
                    .copied()
                    .unwrap_or_default();
                (pem, self.a_venta(&l.partida, pem, gg, bi))
            })
            .collect();
        let c = self.abierta(numero)?;
        for (l, (pem, venta)) in c.lineas.iter_mut().zip(ventas) {
            l.precio = Some(pem);
            l.venta = Some(venta);
        }
        c.cerrada = true;
        Ok(())
    }

    /// Reabre la última certificación (para corregirla). Los precios vuelven
    /// a ser los vigentes.
    pub fn reabrir_certificacion(&mut self, numero: u32) -> Result<(), ErrorMotor> {
        if self.seguimiento.certificaciones.last().map(|c| c.numero) != Some(numero) {
            return Err(error("solo se puede reabrir la última certificación"));
        }
        let c = self.certificacion_mut(numero)?;
        c.cerrada = false;
        for l in &mut c.lineas {
            l.precio = None;
            l.venta = None;
        }
        Ok(())
    }

    /// Borra la última certificación (si está abierta).
    pub fn borrar_certificacion(&mut self, numero: u32) -> Result<(), ErrorMotor> {
        match self.seguimiento.certificaciones.last() {
            Some(c) if c.numero == numero && !c.cerrada => {
                self.seguimiento.certificaciones.pop();
                Ok(())
            }
            Some(c) if c.numero == numero => Err(error("reábrala antes de borrarla")),
            _ => Err(error("solo se puede borrar la última certificación")),
        }
    }

    /// Relación valorada: por cada partida, presupuesto, anterior, mes y a
    /// origen, con subtotales por capítulo.
    pub fn relacion_valorada(&self, numero: u32, precios: Precios) -> Result<Relacion, ErrorMotor> {
        let cert = self
            .certificacion(numero)
            .ok_or_else(|| error(format!("no existe la certificación nº {numero}")))?;
        let previa = self
            .seguimiento
            .certificaciones
            .iter()
            .rev()
            .find(|c| c.numero < numero);
        let v = self.valorar()?;
        let mut filas = Vec::new();
        let mut visitados = Vec::new();
        let total = self.relacion_rec(&self.raiz, 0, cert, previa, &v, precios, &mut filas, &mut visitados)?;
        // Lo certificado en partidas que ya no están en el presupuesto
        // (borradas después) se mantiene, sin presupuesto.
        let mut huerfanas = Importes::default();
        let mut extra = Vec::new();
        for l in cert.lineas.iter().chain(previa.iter().flat_map(|c| c.lineas.iter())) {
            let k = (l.capitulo.clone(), l.partida.clone());
            if visitados.contains(&k) {
                continue;
            }
            visitados.push(k);
            let f = self.fila_partida(&l.capitulo, &l.partida, 1, Decimal::ZERO, None, cert, previa, precios);
            huerfanas.sumar(&f.importes);
            extra.push(f);
        }
        let mut total = total;
        if !extra.is_empty() {
            total.sumar(&huerfanas);
            filas.push(FilaRelacion::Capitulo {
                codigo: String::new(),
                resumen: "Partidas certificadas que ya no están en el presupuesto".into(),
                nivel: 0,
                importes: huerfanas,
            });
            filas.extend(extra.into_iter().map(FilaRelacion::Partida));
        }
        Ok(Relacion {
            numero: cert.numero,
            fecha: cert.fecha.clone(),
            cerrada: cert.cerrada,
            filas,
            total,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn relacion_rec(
        &self,
        capitulo: &str,
        nivel: usize,
        cert: &Certificacion,
        previa: Option<&Certificacion>,
        v: &Valoracion,
        precios: Precios,
        filas: &mut Vec<FilaRelacion>,
        visitados: &mut Vec<(String, String)>,
    ) -> Result<Importes, ErrorMotor> {
        let mut total = Importes::default();
        let Some(lineas) = v.lineas.get(capitulo) else {
            return Ok(total);
        };
        for l in lineas {
            match l.naturaleza {
                Naturaleza::Capitulo => {
                    let pos = filas.len();
                    filas.push(FilaRelacion::Capitulo {
                        codigo: l.hijo.clone(),
                        resumen: self.concepto(&l.hijo)?.resumen.clone(),
                        nivel,
                        importes: Importes::default(),
                    });
                    let i = self.relacion_rec(&l.hijo, nivel + 1, cert, previa, v, precios, filas, visitados)?;
                    if let FilaRelacion::Capitulo { importes, .. } = &mut filas[pos] {
                        *importes = i;
                    }
                    total.sumar(&i);
                }
                Naturaleza::Porcentaje => {}
                _ => {
                    let f = self.fila_partida(
                        capitulo,
                        &l.hijo,
                        nivel,
                        l.cantidad,
                        Some(l.precio),
                        cert,
                        previa,
                        precios,
                    );
                    total.sumar(&f.importes);
                    visitados.push((capitulo.to_owned(), l.hijo.clone()));
                    filas.push(FilaRelacion::Partida(f));
                }
            }
        }
        Ok(total)
    }

    #[allow(clippy::too_many_arguments)]
    fn fila_partida(
        &self,
        capitulo: &str,
        partida: &str,
        nivel: usize,
        medicion: Decimal,
        vigente: Option<Decimal>,
        cert: &Certificacion,
        previa: Option<&Certificacion>,
        precios: Precios,
    ) -> PartidaCertificada {
        let d = self.decimales.importe;
        let congelado = |l: &LineaCertificada| if precios.es_venta() { l.venta } else { l.precio };
        let vigente = vigente.map(|pem| match precios {
            Precios::Ejecucion { .. } => pem,
            Precios::Venta { gg, bi } => self.a_venta(partida, pem, gg, bi),
        });
        let c = self.conceptos.get(partida);
        let la = previa.and_then(|c| c.linea(capitulo, partida));
        let lo = cert.linea(capitulo, partida);
        let cant_anterior = la.map(|l| l.origen).unwrap_or_default();
        let cant_origen = lo.map(|l| l.origen).unwrap_or_default();
        // Precio de este mes: congelado si está cerrada; si no, el vigente
        // (o el de la anterior, si la partida ya no está en el presupuesto)
        let precio = lo
            .and_then(congelado)
            .or(vigente)
            .or_else(|| la.and_then(congelado))
            .unwrap_or_default();
        // Lo anterior, valorado como se certificó; si el precio ha cambiado,
        // la diferencia de lo anterior va al mes (revisión)
        let imp_anterior = la.map_or(Decimal::ZERO, |l| {
            redondear(l.origen * congelado(l).unwrap_or(precio), d)
        });
        let imp_origen = redondear(cant_origen * precio, d);
        PartidaCertificada {
            capitulo: capitulo.to_owned(),
            codigo: partida.to_owned(),
            unidad: c.map(|c| c.unidad.clone()).unwrap_or_default(),
            resumen: c.map(|c| c.resumen.clone()).unwrap_or_default(),
            nivel,
            medicion,
            precio,
            cant_anterior,
            cant_actual: cant_origen - cant_anterior,
            cant_origen,
            importes: Importes {
                presupuesto: redondear(medicion * vigente.unwrap_or_default(), d),
                anterior: imp_anterior,
                actual: imp_origen - imp_anterior,
                origen: imp_origen,
            },
            exceso: cant_origen > medicion,
        }
    }

    /// Precio de venta de una partida a partir de su precio de ejecución.
    fn a_venta(&self, partida: &str, pem: Decimal, gg: Decimal, bi: Decimal) -> Decimal {
        let d = self.decimales.importe;
        match self.conceptos.get(partida) {
            Some(c) => crate::venta::venta_concepto(pem, c, gg, bi, d).venta,
            None => crate::venta::venta_partida(pem, gg, bi, d).venta,
        }
    }

    /// Resumen económico de una relación valorada (IVA en puntos; retención
    /// de garantía en puntos sobre la base del mes). A precios de ejecución
    /// suma GG y BI; a precios de venta ya van dentro.
    pub fn resumen_certificacion(
        &self,
        r: &Relacion,
        precios: Precios,
        iva: Decimal,
        retencion: Decimal,
    ) -> ResumenCertificacion {
        let (gg, bi) = match precios {
            Precios::Ejecucion { gg, bi } => (gg, bi),
            Precios::Venta { .. } => (Decimal::ZERO, Decimal::ZERO),
        };
        let d = self.decimales.importe;
        let anterior = crate::venta::resumen(r.total.anterior, gg, bi, iva, d);
        let origen = crate::venta::resumen(r.total.origen, gg, bi, iva, d);
        // El mes es la diferencia, para que anterior + mes = origen al céntimo
        let actual = crate::venta::ResumenPresupuesto {
            pem: origen.pem - anterior.pem,
            gastos_generales: origen.gastos_generales - anterior.gastos_generales,
            beneficio_industrial: origen.beneficio_industrial - anterior.beneficio_industrial,
            base: origen.base - anterior.base,
            iva: origen.iva - anterior.iva,
            total: origen.total - anterior.total,
        };
        let ret = redondear(actual.base * retencion / Decimal::ONE_HUNDRED, d);
        let a_cobrar = actual.total - ret;
        ResumenCertificacion {
            anterior,
            actual,
            origen,
            retencion: ret,
            a_cobrar,
        }
    }
}

/// Precio con indirectos de cada (capítulo, partida).
fn precios_en_capitulo(v: &Valoracion) -> HashMap<(&str, &str), Decimal> {
    let mut m = HashMap::new();
    for (cap, lineas) in &v.lineas {
        for l in lineas {
            if l.naturaleza != Naturaleza::Capitulo {
                m.insert((cap.as_str(), l.hijo.as_str()), l.precio);
            }
        }
    }
    m
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::LineaDescomposicion as L;
    use rust_decimal_macros::dec;

    const E: Precios = Precios::Ejecucion {
        gg: Decimal::ZERO,
        bi: Decimal::ZERO,
    };

    /// Obra con dos capítulos: C1 (A 10 ud × 5 €, B 4 m × 20 €), C2 (A 2 ud).
    fn obra() -> Presupuesto {
        let mut p = Presupuesto::new("t", "OBRA", "Obra");
        p.insertar(Concepto::capitulo("C1", "Uno")).unwrap();
        p.insertar(Concepto::capitulo("C2", "Dos")).unwrap();
        let mut a = Concepto::partida("A", "ud", "Partida A", vec![]);
        a.precio = dec!(5);
        let mut b = Concepto::partida("B", "m", "Partida B", vec![]);
        b.precio = dec!(20);
        p.insertar(a).unwrap();
        p.insertar(b).unwrap();
        for c in ["C1", "C2"] {
            p.colgar("OBRA", c, None).unwrap();
        }
        p.colgar("C1", "A", None).unwrap();
        p.colgar("C1", "B", None).unwrap();
        p.colgar("C2", "A", None).unwrap();
        cantidad(&mut p, "C1", "A", dec!(10));
        cantidad(&mut p, "C1", "B", dec!(4));
        cantidad(&mut p, "C2", "A", dec!(2));
        p
    }

    fn cantidad(p: &mut Presupuesto, padre: &str, hijo: &str, c: Decimal) {
        let l = p
            .conceptos
            .get_mut(padre)
            .unwrap()
            .descomposicion
            .iter_mut()
            .find(|l| l.hijo == hijo)
            .unwrap();
        *l = L::new(hijo, c);
    }

    #[test]
    fn contradictorios_y_resumen_del_contrato() {
        let mut p = obra();
        assert_eq!(p.pem().unwrap(), dec!(140));
        let c = p
            .nuevo_contradictorio("Rejillas no previstas", "01/10/2026", "Orden DF")
            .unwrap();
        assert_eq!(c, "CTR-01");
        p.colgar(&c, "B", None).unwrap();
        cantidad(&mut p, &c, "B", dec!(2));
        let c2 = p.nuevo_contradictorio("Otro", "02/10/2026", "").unwrap();
        assert_eq!(c2, "CTR-02");
        assert_eq!(p.seguimiento.contradictorios[&c2].numero, 2);
        p.colgar(&c2, "A", None).unwrap();
        let r = p.resumen_contrato().unwrap();
        assert_eq!(r.contratado, dec!(140));
        assert_eq!(r.pendientes, dec!(45));
        p.fijar_estado_contradictorio(&c, EstadoContradictorio::Aprobado)
            .unwrap();
        p.fijar_estado_contradictorio(&c2, EstadoContradictorio::Rechazado)
            .unwrap();
        let r = p.resumen_contrato().unwrap();
        assert_eq!((r.aprobados, r.rechazados, r.vigente), (dec!(40), dec!(5), dec!(180)));
        // Para firmar: solo ese capítulo
        let s = p.solo_contradictorio(&c).unwrap();
        assert_eq!(s.pem().unwrap(), dec!(40));
        assert!(p.solo_contradictorio("C1").is_err());
        assert!(
            p.marcar_contradictorio("A", Some(p.seguimiento.contradictorios[&c].clone()))
                .is_err()
        );
        p.marcar_contradictorio(&c2, None).unwrap();
        assert_eq!(p.resumen_contrato().unwrap().contratado, dec!(145));
    }

    #[test]
    fn certificaciones_a_origen_y_del_mes() {
        let mut p = obra();
        assert!(p.relacion_valorada(1, E).is_err());
        let n = p.nueva_certificacion("31/10/2026").unwrap();
        assert_eq!(n, 1);
        assert!(p.nueva_certificacion("x").is_err(), "solo una abierta");
        p.certificar_origen(1, "C1", "A", dec!(4)).unwrap();
        p.certificar_mes(1, "C1", "B", dec!(1.5)).unwrap();
        assert!(p.certificar_origen(1, "C2", "B", dec!(1)).is_err(), "B no está en C2");
        let r = p.relacion_valorada(1, E).unwrap();
        assert_eq!(r.total.presupuesto, dec!(140));
        assert_eq!(r.total.origen, dec!(50));
        assert_eq!(r.total.actual, dec!(50));
        assert_eq!(r.total.porcentaje(), dec!(35.71));
        p.cerrar_certificacion(1, dec!(0), dec!(0)).unwrap();
        assert!(p.certificar_origen(1, "C1", "A", dec!(5)).is_err(), "cerrada");
        // Cambia el precio de A: la 1ª no cambia
        p.fijar_precio("A", dec!(6)).unwrap();
        assert_eq!(p.relacion_valorada(1, E).unwrap().total.origen, dec!(50));
        let n2 = p.nueva_certificacion("30/11/2026").unwrap();
        assert_eq!(p.certificacion(n2).unwrap().origen("C1", "A"), dec!(4));
        p.certificar_mes(n2, "C1", "A", dec!(3)).unwrap();
        p.certificar_completa(n2, "C2", "A").unwrap();
        let r = p.relacion_valorada(n2, E).unwrap();
        let a = r.partidas().find(|f| f.capitulo == "C1" && f.codigo == "A").unwrap();
        assert_eq!(
            (a.cant_anterior, a.cant_actual, a.cant_origen),
            (dec!(4), dec!(3), dec!(7))
        );
        // Anterior a 5 €, origen a 6 €: el mes recoge la revisión
        assert_eq!(
            (a.importes.anterior, a.importes.origen, a.importes.actual),
            (dec!(20), dec!(42), dec!(22))
        );
        let b = r.partidas().find(|f| f.codigo == "B").unwrap();
        assert_eq!(
            (b.importes.anterior, b.importes.actual, b.importes.origen),
            (dec!(30), dec!(0), dec!(30))
        );
        assert_eq!(r.total.anterior + r.total.actual, r.total.origen);
        assert!(r.excesos().is_empty());
        p.certificar_origen(n2, "C2", "A", dec!(3)).unwrap();
        assert_eq!(p.relacion_valorada(n2, E).unwrap().excesos().len(), 1);
        // Capítulos con subtotal
        let caps: Vec<_> = r
            .filas
            .iter()
            .filter_map(|f| match f {
                FilaRelacion::Capitulo { codigo, importes, .. } => Some((codigo.clone(), importes.origen)),
                _ => None,
            })
            .collect();
        assert_eq!(caps, vec![("C1".into(), dec!(72)), ("C2".into(), dec!(12))]);
        // Reabrir y borrar solo la última
        assert!(p.reabrir_certificacion(1).is_err());
        assert!(p.borrar_certificacion(1).is_err());
        p.borrar_certificacion(n2).unwrap();
        p.reabrir_certificacion(1).unwrap();
        assert_eq!(
            p.relacion_valorada(1, E).unwrap().total.origen,
            dec!(54),
            "precio vigente"
        );
    }

    #[test]
    fn resumen_economico_y_partidas_borradas() {
        let mut p = obra();
        p.nueva_certificacion("1").unwrap();
        p.certificar_origen(1, "C1", "B", dec!(4)).unwrap();
        p.cerrar_certificacion(1, dec!(0), dec!(0)).unwrap();
        p.nueva_certificacion("2").unwrap();
        p.certificar_origen(2, "C1", "A", dec!(10)).unwrap();
        let pr = Precios::Ejecucion {
            gg: dec!(13),
            bi: dec!(6),
        };
        let s = p.resumen_certificacion(&p.relacion_valorada(2, pr).unwrap(), pr, dec!(21), dec!(5));
        assert_eq!(s.anterior.pem, dec!(80));
        assert_eq!(s.actual.pem, dec!(50));
        assert_eq!(s.origen.pem, dec!(130));
        assert_eq!(s.anterior.base + s.actual.base, s.origen.base);
        assert_eq!(s.actual.base, dec!(59.50));
        assert_eq!(s.retencion, dec!(2.98));
        assert_eq!(s.a_cobrar, s.actual.total - s.retencion);
        // Se quita B del capítulo: lo certificado no se pierde
        let c1 = p.conceptos.get_mut("C1").unwrap();
        c1.descomposicion.retain(|l| l.hijo != "B");
        let r = p.relacion_valorada(2, E).unwrap();
        assert_eq!(r.total.anterior, dec!(80));
        assert!(
            r.partidas()
                .any(|f| f.codigo == "B" && f.importes.presupuesto.is_zero())
        );
        assert_eq!(r.total.anterior + r.total.actual, r.total.origen);
    }

    #[test]
    fn a_precios_de_venta() {
        let mut p = obra();
        p.fijar_margen("B", Some(dec!(50))).unwrap();
        let v = Precios::Venta {
            gg: dec!(10),
            bi: dec!(10),
        };
        p.nueva_certificacion("1").unwrap();
        p.certificar_origen(1, "C1", "A", dec!(1)).unwrap();
        p.certificar_origen(1, "C1", "B", dec!(1)).unwrap();
        let r = p.relacion_valorada(1, v).unwrap();
        // A: 5 + 10 % + 10 % = 6; B: 20 + 50 % = 30
        assert_eq!(r.total.origen, dec!(36));
        assert_eq!(r.total.presupuesto, dec!(60) + dec!(120) + dec!(12));
        let s = p.resumen_certificacion(&r, v, dec!(21), Decimal::ZERO);
        assert_eq!((s.actual.base, s.actual.iva), (dec!(36), dec!(7.56)));
        // Cerrada: congela la venta aunque luego cambie el margen
        p.cerrar_certificacion(1, dec!(10), dec!(10)).unwrap();
        p.fijar_margen("B", None).unwrap();
        assert_eq!(p.relacion_valorada(1, v).unwrap().total.origen, dec!(36));
    }

    #[test]
    fn viaja_en_json() {
        let mut p = obra();
        p.nuevo_contradictorio("X", "1", "n").unwrap();
        p.nueva_certificacion("1").unwrap();
        p.certificar_origen(1, "C1", "A", dec!(2)).unwrap();
        p.cerrar_certificacion(1, dec!(0), dec!(0)).unwrap();
        let j = serde_json::to_string(&p).unwrap();
        let q: Presupuesto = serde_json::from_str(&j).unwrap();
        assert_eq!(q, p);
    }
}
