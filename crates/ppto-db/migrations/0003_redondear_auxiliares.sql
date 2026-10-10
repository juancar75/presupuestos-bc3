-- SPDX-License-Identifier: GPL-3.0-or-later
-- Opción de Presto «Redondear partidas que actúan como auxiliares» (desmarcada por defecto).
ALTER TABLE revisiones ADD COLUMN redondear_auxiliares INTEGER NOT NULL DEFAULT 0;
