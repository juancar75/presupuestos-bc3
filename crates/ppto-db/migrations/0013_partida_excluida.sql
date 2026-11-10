-- SPDX-License-Identifier: GPL-3.0-or-later
-- Partida excluida de la oferta (importe cero, sale como «EXCLUIDO»).
ALTER TABLE conceptos ADD COLUMN excluida INTEGER NOT NULL DEFAULT 0;
