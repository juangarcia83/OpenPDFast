// SPDX-License-Identifier: AGPL-3.0-or-later

use mupdf_sys::{fz_irect, fz_matrix, fz_rect};

/// Rectangle in PDF points (or device pixels once transformed).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Rect {
    pub const fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> f32 {
        self.y1 - self.y0
    }

    pub fn is_empty(&self) -> bool {
        !(self.x1 > self.x0 && self.y1 > self.y0)
    }
}

impl From<fz_rect> for Rect {
    fn from(r: fz_rect) -> Self {
        Self::new(r.x0, r.y0, r.x1, r.y1)
    }
}

impl From<Rect> for fz_rect {
    fn from(r: Rect) -> Self {
        fz_rect {
            x0: r.x0,
            y0: r.y0,
            x1: r.x1,
            y1: r.y1,
        }
    }
}

/// Integer rectangle in device pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct IRect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl IRect {
    pub const fn new(x0: i32, y0: i32, x1: i32, y1: i32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    pub fn width(&self) -> i32 {
        self.x1 - self.x0
    }

    pub fn height(&self) -> i32 {
        self.y1 - self.y0
    }

    pub fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }
}

impl From<IRect> for fz_irect {
    fn from(r: IRect) -> Self {
        fz_irect {
            x0: r.x0,
            y0: r.y0,
            x1: r.x1,
            y1: r.y1,
        }
    }
}

impl From<IRect> for Rect {
    fn from(r: IRect) -> Self {
        Rect::new(r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32)
    }
}

/// Affine transform `[a b c d e f]`, as in PDF.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Matrix {
    pub const IDENTITY: Self = Self::scale(1.0, 1.0);

    pub const fn scale(sx: f32, sy: f32) -> Self {
        Self {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            e: 0.0,
            f: 0.0,
        }
    }
}

impl From<Matrix> for fz_matrix {
    fn from(m: Matrix) -> Self {
        fz_matrix {
            a: m.a,
            b: m.b,
            c: m.c,
            d: m.d,
            e: m.e,
            f: m.f,
        }
    }
}
