// SPDX-License-Identifier: GPL-3.0-or-later
//! **Pedidos registrados** de la obra y control de compras.
//!
//! Los pedidos que salen de Informes → Pedido a fabricante o subcontrata se
//! pueden **registrar** en la obra: quedan con número, fecha, estado
//! (borrador, enviado, anulado) y lo recibido de cada línea. Al registrar un
//! pedido nuevo al mismo proveedor solo se propone lo que falta por pedir.
//!
//! El [`control`] compara, recurso a recurso, lo presupuestado, lo necesario
//! según lo certificado a origen, lo pedido y lo recibido, y avisa de lo que
//! se pide de más.

use crate::concepto::Naturaleza;
use crate::error::ErrorMotor;
use crate::pedidos::{Pedido, TipoPedido, proveedor};
use crate::presupuesto::Presupuesto;
use crate::{Decimal, redondear};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EstadoPedido {
    /// Se puede cambiar.
    #[default]
    Borrador,
    Enviado,
    Anulado,
}

impl EstadoPedido {
    pub const TODOS: [Self; 3] = [Self::Borrador, Self::Enviado, Self::Anulado];

    pub fn nombre(self) -> &'static str {
        match self {
            Self::Borrador => "Borrador",
            Self::Enviado => "Enviado",
            Self::Anulado => "Anulado",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineaRegistrada {
    pub codigo: String,
    pub resumen: String,
    pub unidad: String,
    pub cantidad: Decimal,
    pub precio: Decimal,
    #[serde(default)]
    pub recibido: Decimal,
}

impl LineaRegistrada {
    pub fn importe(&self) -> Decimal {
        redondear(self.cantidad * self.precio, 2)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PedidoRegistrado {
    pub numero: u32,
    pub proveedor: String,
    pub material: bool,
    pub fecha: String,
    #[serde(default)]
    pub referencia: String,
    /// Fecha de entrega prevista (texto libre).
    #[serde(default)]
    pub entrega: String,
    pub estado: EstadoPedido,
    pub lineas: Vec<LineaRegistrada>,
    #[serde(default)]
    pub nota: String,
}

impl PedidoRegistrado {
    pub fn total(&self) -> Decimal {
        self.lineas.iter().map(LineaRegistrada::importe).sum()
    }

    /// Situación para mostrar: borrador, enviado, recibido parcial, recibido, anulado.
    pub fn situacion(&self) -> &'static str {
        match self.estado {
            EstadoPedido::Borrador => "Borrador",
            EstadoPedido::Anulado => "Anulado",
            EstadoPedido::Enviado => {
                let alguno = self.lineas.iter().any(|l| !l.recibido.is_zero());
                let todo = self.lineas.iter().all(|l| l.recibido >= l.cantidad);
                match (alguno, todo) {
                    (_, true) => "Recibido",
                    (true, false) => "Recibido en parte",
                    _ => "Enviado",
                }
            }
        }
    }

    pub fn nombre(&self) -> String {
        if self.proveedor.is_empty() {
            "(sin proveedor)".into()
        } else {
            self.proveedor.clone()
        }
    }

    /// Como pedido para imprimirlo (PDF, Excel).
    pub fn como_pedido(&self) -> Pedido {
        let lineas: Vec<crate::pedidos::LineaPedido> = self
            .lineas
            .iter()
            .map(|l| crate::pedidos::LineaPedido {
                codigo: l.codigo.clone(),
                referencia: None,
                resumen: l.resumen.clone(),
                unidad: l.unidad.clone(),
                cantidad: l.cantidad,
                pvp: None,
                descuento: None,
                precio: l.precio,
                importe: l.importe(),
                oferta: None,
                partidas: Vec::new(),
            })
            .collect();
        Pedido {
            proveedor: self.proveedor.clone(),
            tipo: if self.material {
                TipoPedido::Material
            } else {
                TipoPedido::Subcontrata
            },
            total: lineas.iter().map(|l| l.importe).sum(),
            lineas,
        }
    }
}

/// Un recurso en el control de compras.
#[derive(Debug, Clone, PartialEq)]
pub struct ControlRecurso {
    pub codigo: String,
    pub resumen: String,
    pub unidad: String,
    pub proveedor: String,
    pub presupuestado: Decimal,
    /// Necesario para lo certificado a origen (última certificación).
    pub certificado: Decimal,
    /// En pedidos no anulados.
    pub pedido: Decimal,
    pub recibido: Decimal,
}

impl ControlRecurso {
    pub fn por_pedir(&self) -> Decimal {
        (self.presupuestado - self.pedido).max(Decimal::ZERO)
    }

    pub fn por_recibir(&self) -> Decimal {
        (self.pedido - self.recibido).max(Decimal::ZERO)
    }

    /// Se ha pedido más de lo presupuestado.
    pub fn exceso(&self) -> bool {
        self.pedido > self.presupuestado
    }
}

fn error(m: impl Into<String>) -> ErrorMotor {
    ErrorMotor::ValorInvalido(m.into())
}

impl Presupuesto {
    fn pedido_mut(&mut self, numero: u32) -> Result<&mut PedidoRegistrado, ErrorMotor> {
        self.seguimiento
            .pedidos
            .iter_mut()
            .find(|p| p.numero == numero)
            .ok_or_else(|| error(format!("no existe el pedido nº {numero}")))
    }

    pub fn pedido_registrado(&self, numero: u32) -> Option<&PedidoRegistrado> {
        self.seguimiento.pedidos.iter().find(|p| p.numero == numero)
    }

    /// Cantidad ya pedida de cada recurso (pedidos no anulados).
    pub fn ya_pedido(&self) -> BTreeMap<String, Decimal> {
        let mut m: BTreeMap<String, Decimal> = BTreeMap::new();
        for p in &self.seguimiento.pedidos {
            if p.estado == EstadoPedido::Anulado {
                continue;
            }
            for l in &p.lineas {
                *m.entry(l.codigo.clone()).or_default() += l.cantidad;
            }
        }
        m
    }

    /// Registra un pedido (en borrador) con lo que **falta por pedir** de
    /// cada línea; las que ya están pedidas del todo no entran. Devuelve el
    /// número, o `None` si no queda nada por pedir.
    pub fn registrar_pedido(&mut self, x: &Pedido, fecha: &str, referencia: &str) -> Option<u32> {
        let ya = self.ya_pedido();
        let lineas: Vec<LineaRegistrada> = x
            .lineas
            .iter()
            .filter_map(|l| {
                let falta = l.cantidad - ya.get(&l.codigo).copied().unwrap_or_default();
                (falta > Decimal::ZERO).then(|| LineaRegistrada {
                    codigo: l.codigo.clone(),
                    resumen: l.resumen.clone(),
                    unidad: l.unidad.clone(),
                    cantidad: falta,
                    precio: l.precio,
                    recibido: Decimal::ZERO,
                })
            })
            .collect();
        if lineas.is_empty() {
            return None;
        }
        let numero = self.seguimiento.pedidos.iter().map(|p| p.numero).max().unwrap_or(0) + 1;
        self.seguimiento.pedidos.push(PedidoRegistrado {
            numero,
            proveedor: x.proveedor.clone(),
            material: x.tipo == TipoPedido::Material,
            fecha: fecha.to_owned(),
            referencia: referencia.to_owned(),
            entrega: String::new(),
            estado: EstadoPedido::Borrador,
            lineas,
            nota: String::new(),
        });
        Some(numero)
    }

    pub fn fijar_estado_pedido(&mut self, numero: u32, estado: EstadoPedido) -> Result<(), ErrorMotor> {
        self.pedido_mut(numero)?.estado = estado;
        Ok(())
    }

    /// Cambia los datos de cabecera (fecha, referencia, entrega, nota).
    pub fn fijar_datos_pedido(
        &mut self,
        numero: u32,
        fecha: &str,
        referencia: &str,
        entrega: &str,
        nota: &str,
    ) -> Result<(), ErrorMotor> {
        let p = self.pedido_mut(numero)?;
        p.fecha = fecha.to_owned();
        p.referencia = referencia.to_owned();
        p.entrega = entrega.to_owned();
        p.nota = nota.to_owned();
        Ok(())
    }

    /// Cantidad pedida de una línea (solo en borrador; 0 la quita).
    pub fn fijar_cantidad_pedido(&mut self, numero: u32, codigo: &str, cantidad: Decimal) -> Result<(), ErrorMotor> {
        if cantidad < Decimal::ZERO {
            return Err(error("la cantidad no puede ser negativa"));
        }
        let p = self.pedido_mut(numero)?;
        if p.estado != EstadoPedido::Borrador {
            return Err(error(format!(
                "el pedido nº {numero} ya está {}",
                p.estado.nombre().to_lowercase()
            )));
        }
        let l = p
            .lineas
            .iter_mut()
            .find(|l| l.codigo == codigo)
            .ok_or_else(|| error(format!("{codigo} no está en el pedido nº {numero}")))?;
        l.cantidad = cantidad;
        p.lineas.retain(|l| !l.cantidad.is_zero());
        Ok(())
    }

    /// Cantidad recibida de una línea (pedido enviado).
    pub fn recibir(&mut self, numero: u32, codigo: &str, recibido: Decimal) -> Result<(), ErrorMotor> {
        if recibido < Decimal::ZERO {
            return Err(error("lo recibido no puede ser negativo"));
        }
        let p = self.pedido_mut(numero)?;
        if p.estado != EstadoPedido::Enviado {
            return Err(error("solo se recibe material de pedidos enviados"));
        }
        p.lineas
            .iter_mut()
            .find(|l| l.codigo == codigo)
            .ok_or_else(|| error(format!("{codigo} no está en el pedido nº {numero}")))?
            .recibido = recibido;
        Ok(())
    }

    /// Todo recibido.
    pub fn recibir_todo(&mut self, numero: u32) -> Result<(), ErrorMotor> {
        let p = self.pedido_mut(numero)?;
        if p.estado != EstadoPedido::Enviado {
            return Err(error("solo se recibe material de pedidos enviados"));
        }
        for l in &mut p.lineas {
            l.recibido = l.cantidad;
        }
        Ok(())
    }

    /// Borra un pedido en borrador.
    pub fn borrar_pedido(&mut self, numero: u32) -> Result<(), ErrorMotor> {
        let p = self.pedido_mut(numero)?;
        if p.estado != EstadoPedido::Borrador {
            return Err(error("solo se borran pedidos en borrador; anúlelo"));
        }
        self.seguimiento.pedidos.retain(|p| p.numero != numero);
        Ok(())
    }

    /// Cantidades de cada partida certificadas a origen (última certificación).
    pub fn certificado_por_partida(&self) -> BTreeMap<String, Decimal> {
        let mut m: BTreeMap<String, Decimal> = BTreeMap::new();
        if let Some(c) = self.seguimiento.certificaciones.last() {
            for l in &c.lineas {
                if self.conceptos.contains_key(&l.partida) {
                    *m.entry(l.partida.clone()).or_default() += l.origen;
                }
            }
        }
        m
    }

    /// Control de compras por recurso (material, maquinaria, otros y
    /// subcontratas), incluidos los pedidos de recursos que ya no están en el
    /// presupuesto.
    pub fn control_compras(&self) -> Result<Vec<ControlRecurso>, ErrorMotor> {
        let e = self.explosion_recursos()?;
        let cert = self.explosion_de(&self.certificado_por_partida())?;
        let cert: BTreeMap<&str, Decimal> = cert.recursos.iter().map(|r| (r.codigo.as_str(), r.cantidad)).collect();
        let mut out: BTreeMap<String, ControlRecurso> = BTreeMap::new();
        for r in &e.recursos {
            if !matches!(
                r.naturaleza,
                Naturaleza::Material | Naturaleza::Maquinaria | Naturaleza::Otros | Naturaleza::Subcontrata
            ) {
                continue;
            }
            out.insert(
                r.codigo.clone(),
                ControlRecurso {
                    codigo: r.codigo.clone(),
                    resumen: r.resumen.clone(),
                    unidad: r.unidad.clone(),
                    proveedor: proveedor(self, &r.codigo),
                    presupuestado: r.cantidad,
                    certificado: cert.get(r.codigo.as_str()).copied().unwrap_or_default(),
                    pedido: Decimal::ZERO,
                    recibido: Decimal::ZERO,
                },
            );
        }
        for p in &self.seguimiento.pedidos {
            if p.estado == EstadoPedido::Anulado {
                continue;
            }
            for l in &p.lineas {
                let c = out.entry(l.codigo.clone()).or_insert_with(|| ControlRecurso {
                    codigo: l.codigo.clone(),
                    resumen: l.resumen.clone(),
                    unidad: l.unidad.clone(),
                    proveedor: p.proveedor.clone(),
                    presupuestado: Decimal::ZERO,
                    certificado: Decimal::ZERO,
                    pedido: Decimal::ZERO,
                    recibido: Decimal::ZERO,
                });
                c.pedido += l.cantidad;
                c.recibido += l.recibido;
            }
        }
        Ok(out.into_values().collect())
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::ejemplos::suelo_radiante;
    use rust_decimal_macros::dec;

    #[test]
    fn registrar_enviar_recibir_y_controlar() {
        let mut p = suelo_radiante();
        let pedidos = crate::pedidos::pedidos(&p).unwrap();
        let x = pedidos.iter().find(|x| x.tipo == TipoPedido::Material).unwrap().clone();
        let n = p.registrar_pedido(&x, "01/11/2026", "OBRA-1").unwrap();
        assert_eq!(n, 1);
        let reg = p.pedido_registrado(1).unwrap().clone();
        assert_eq!(reg.lineas.len(), x.lineas.len());
        assert_eq!(reg.total(), x.total);
        assert_eq!(reg.situacion(), "Borrador");
        // Todo pedido: un segundo pedido no tiene nada
        assert!(p.registrar_pedido(&x, "x", "").is_none());
        // Pedir solo la mitad de la primera línea: el siguiente propone el resto
        let l0 = reg.lineas[0].clone();
        let mitad = redondear(l0.cantidad / Decimal::TWO, 2);
        p.fijar_cantidad_pedido(1, &l0.codigo, mitad).unwrap();
        let n2 = p.registrar_pedido(&x, "02/11/2026", "").unwrap();
        let r2 = p.pedido_registrado(n2).unwrap();
        assert_eq!(r2.lineas.len(), 1);
        assert_eq!(r2.lineas[0].cantidad, l0.cantidad - mitad);
        // Enviar y recibir
        assert!(p.recibir(1, &l0.codigo, dec!(1)).is_err(), "en borrador no se recibe");
        p.fijar_estado_pedido(1, EstadoPedido::Enviado).unwrap();
        assert!(
            p.fijar_cantidad_pedido(1, &l0.codigo, dec!(1)).is_err(),
            "enviado: no se cambia"
        );
        p.recibir(1, &l0.codigo, mitad).unwrap();
        assert_eq!(p.pedido_registrado(1).unwrap().situacion(), "Recibido en parte");
        p.recibir_todo(1).unwrap();
        assert_eq!(p.pedido_registrado(1).unwrap().situacion(), "Recibido");
        assert!(p.borrar_pedido(1).is_err());
        p.borrar_pedido(n2).unwrap();
        // Control
        let c = p.control_compras().unwrap();
        let r = c.iter().find(|r| r.codigo == l0.codigo).unwrap();
        assert_eq!(
            (r.pedido, r.recibido, r.por_pedir()),
            (mitad, mitad, l0.cantidad - mitad)
        );
        assert!(!r.exceso());
        // Anulado: no cuenta
        p.fijar_estado_pedido(1, EstadoPedido::Anulado).unwrap();
        assert!(p.ya_pedido().is_empty());
        assert_eq!(
            p.pedido_registrado(1).unwrap().como_pedido().lineas.len(),
            reg.lineas.len()
        );
    }

    #[test]
    fn exceso_y_certificado() {
        let mut p = suelo_radiante();
        let x = crate::pedidos::pedidos(&p)
            .unwrap()
            .into_iter()
            .find(|x| x.tipo == TipoPedido::Material)
            .unwrap();
        p.registrar_pedido(&x, "", "").unwrap();
        let l0 = p.pedido_registrado(1).unwrap().lineas[0].clone();
        p.fijar_cantidad_pedido(1, &l0.codigo, l0.cantidad + dec!(10)).unwrap();
        // Certificar el 100 % de la primera partida del capítulo
        let (cap, partida) = ("C01".to_owned(), "SR.M2".to_owned());
        let n = p.nueva_certificacion("1").unwrap();
        p.certificar_completa(n, &cap, &partida).unwrap();
        let c = p.control_compras().unwrap();
        let r = c.iter().find(|r| r.codigo == l0.codigo).unwrap();
        assert!(r.exceso());
        assert_eq!(r.por_pedir(), Decimal::ZERO);
        assert!(c.iter().any(|r| r.certificado > Decimal::ZERO));
        // JSON con pedidos
        let q: Presupuesto = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(q, p);
    }
}
