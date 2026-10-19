// SPDX-License-Identifier: GPL-3.0-or-later
//! Perfiles de empresa para el informe: cada empresa o tipo de actividad
//! (instaladora, ingeniería, obra pública...) guarda su logo, sus datos, su
//! firma, sus observaciones y cómo se presenta el resumen (con o sin PEM,
//! gastos generales y beneficio industrial).

use crate::{ErrorPdf, OpcionesPdf};
use ppto_core::{Decimal, venta};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Observaciones tipo para una instaladora (oferta de suministro y montaje).
pub const OBSERVACIONES_INSTALADORA: [&str; 7] = [
    "Oferta válida durante 30 días desde la fecha de emisión.",
    "Incluye suministro, montaje, pruebas y puesta en marcha de los equipos y materiales indicados.",
    "No incluye ayudas de albañilería, obra civil, acometidas eléctricas hasta los equipos, tasas ni legalizaciones, salvo que figuren expresamente.",
    "Las mediciones son estimadas; se facturará según la medición real ejecutada a los precios unitarios ofertados.",
    "Los plazos de entrega de equipos están sujetos a la disponibilidad del fabricante en el momento del pedido.",
    "Garantía de los equipos según fabricante; garantía de la instalación: un año desde la puesta en marcha.",
    "Forma de pago: a convenir.",
];

/// Configuración del informe de una empresa o tipo de actividad.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Perfil {
    pub nombre: String,
    pub tipo_documento: String,
    pub empresa: String,
    pub empresa_datos: String,
    pub autor: String,
    pub autor_datos: String,
    pub lugar: String,
    /// Copia del logo guardada junto a los perfiles (ver [`Perfiles::importar_logo`]).
    pub logo: Option<PathBuf>,
    pub observaciones: Vec<String>,
    /// Resumen con PEM + GG + BI (`true`) o solo suma, IVA y total (`false`).
    pub desglose_contrata: bool,
    /// Instaladora: GG y BI dentro del precio de cada partida (precios de venta).
    pub precios_venta: bool,
    pub gastos_generales: Decimal,
    pub beneficio_industrial: Decimal,
    pub iva: Decimal,
    pub mostrar_mediciones: bool,
    pub mostrar_textos: bool,
    pub plantilla: Option<PathBuf>,
}

impl Default for Perfil {
    fn default() -> Self {
        Self::instaladora()
    }
}

impl Perfil {
    /// Instaladora: oferta sin PEM, GG ni BI (el margen va en los precios).
    pub fn instaladora() -> Self {
        Self {
            nombre: "Instaladora".into(),
            tipo_documento: "Oferta".into(),
            empresa: String::new(),
            empresa_datos: String::new(),
            autor: String::new(),
            autor_datos: String::new(),
            lugar: String::new(),
            logo: None,
            observaciones: OBSERVACIONES_INSTALADORA.iter().map(|s| (*s).to_owned()).collect(),
            desglose_contrata: false,
            precios_venta: true,
            gastos_generales: Decimal::ZERO,
            beneficio_industrial: Decimal::ZERO,
            iva: venta::IVA_GENERAL,
            mostrar_mediciones: true,
            mostrar_textos: true,
            plantilla: None,
        }
    }

    /// Ingeniería / proyecto: PEM, 13 % GG, 6 % BI e IVA.
    pub fn ingenieria() -> Self {
        Self {
            nombre: "Ingeniería (PEM + GG + BI)".into(),
            tipo_documento: "Presupuesto".into(),
            observaciones: crate::OBSERVACIONES_HABITUALES
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            desglose_contrata: true,
            precios_venta: false,
            gastos_generales: venta::GG_HABITUAL,
            beneficio_industrial: venta::BI_HABITUAL,
            ..Self::instaladora()
        }
    }

    /// Copia los datos del perfil en las opciones del informe (los datos de
    /// la obra —título, cliente, fecha, revisión...— no se tocan).
    pub fn aplicar(&self, o: &mut OpcionesPdf) {
        o.tipo_documento.clone_from(&self.tipo_documento);
        o.empresa.clone_from(&self.empresa);
        o.empresa_datos.clone_from(&self.empresa_datos);
        o.autor.clone_from(&self.autor);
        o.autor_datos.clone_from(&self.autor_datos);
        o.lugar.clone_from(&self.lugar);
        o.logo.clone_from(&self.logo);
        o.observaciones.clone_from(&self.observaciones);
        o.desglose_contrata = self.desglose_contrata;
        o.precios_venta = self.precios_venta;
        o.gastos_generales = self.gastos_generales;
        o.beneficio_industrial = self.beneficio_industrial;
        o.iva = self.iva;
        o.mostrar_mediciones = self.mostrar_mediciones;
        o.mostrar_textos = self.mostrar_textos;
        o.plantilla.clone_from(&self.plantilla);
    }
}

/// Lista de perfiles y el elegido. Se guarda como JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Perfiles {
    pub activo: usize,
    pub lista: Vec<Perfil>,
}

impl Default for Perfiles {
    fn default() -> Self {
        Self {
            activo: 0,
            lista: vec![Perfil::instaladora(), Perfil::ingenieria()],
        }
    }
}

impl Perfiles {
    pub fn activo(&self) -> &Perfil {
        &self.lista[self.activo.min(self.lista.len() - 1)]
    }

    pub fn activo_mut(&mut self) -> &mut Perfil {
        let i = self.activo.min(self.lista.len() - 1);
        &mut self.lista[i]
    }

    /// Añade una copia del perfil activo con otro nombre y la deja activa.
    pub fn duplicar(&mut self) {
        let mut p = self.activo().clone();
        p.nombre = format!("{} (copia)", p.nombre);
        self.lista.push(p);
        self.activo = self.lista.len() - 1;
    }

    /// Borra el perfil activo (siempre queda al menos uno).
    pub fn borrar_activo(&mut self) {
        if self.lista.len() > 1 {
            self.lista.remove(self.activo.min(self.lista.len() - 1));
            self.activo = 0;
        }
    }

    /// Lee los perfiles; si no hay fichero o está dañado, los de serie.
    pub fn cargar(ruta: &Path) -> Self {
        let mut p: Self = std::fs::read(ruta)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if p.lista.is_empty() {
            p = Self::default();
        }
        p
    }

    pub fn guardar(&self, ruta: &Path) -> std::io::Result<()> {
        if let Some(dir) = ruta.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(ruta, json)
    }

    /// Copia el logo elegido a `dir/logos/` y lo asigna al perfil activo, de
    /// modo que el informe no dependa de que el original siga en su sitio.
    /// Admite PNG, JPG, GIF y SVG de hasta 5 MB.
    pub fn importar_logo(&mut self, origen: &Path, dir: &Path) -> Result<PathBuf, ErrorPdf> {
        let ext = origen
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .filter(|e| ["png", "jpg", "jpeg", "gif", "svg"].contains(&e.as_str()))
            .ok_or_else(|| ErrorPdf::FormatoLogo(origen.to_owned()))?;
        let bytes = std::fs::read(origen).map_err(|e| ErrorPdf::Fichero(origen.to_owned(), e))?;
        if bytes.len() > 5 * 1024 * 1024 {
            return Err(ErrorPdf::FormatoLogo(origen.to_owned()));
        }
        let nombre: String = self
            .activo()
            .nombre
            .chars()
            .map(|c| {
                if c.is_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        let destino = dir.join("logos").join(format!("{nombre}.{ext}"));
        std::fs::create_dir_all(dir.join("logos")).map_err(ErrorPdf::Escritura)?;
        std::fs::write(&destino, bytes).map_err(ErrorPdf::Escritura)?;
        self.activo_mut().logo = Some(destino.clone());
        Ok(destino)
    }
}
