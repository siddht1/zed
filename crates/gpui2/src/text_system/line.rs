use crate::{
    black, point, px, BorrowWindow, Bounds, Hsla, LineLayout, Pixels, Point, Result, SharedString,
    UnderlineStyle, WindowContext, WrapBoundary, WrappedLineLayout,
};
use derive_more::{Deref, DerefMut};
use smallvec::SmallVec;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct DecorationRun {
    pub len: u32,
    pub color: Hsla,
    pub underline: Option<UnderlineStyle>,
}

#[derive(Clone, Default, Debug, Deref, DerefMut)]
pub struct ShapedLine {
    #[deref]
    #[deref_mut]
    pub(crate) layout: Arc<LineLayout>,
    pub text: SharedString,
    pub(crate) decoration_runs: SmallVec<[DecorationRun; 32]>,
}

impl ShapedLine {
    pub fn len(&self) -> usize {
        self.layout.len
    }

    pub fn paint(
        &self,
        origin: Point<Pixels>,
        line_height: Pixels,
        cx: &mut WindowContext,
    ) -> Result<()> {
        paint_line(
            origin,
            &self.layout,
            line_height,
            &self.decoration_runs,
            None,
            &[],
            cx,
        )?;

        Ok(())
    }
}

#[derive(Clone, Default, Debug, Deref, DerefMut)]
pub struct WrappedLine {
    #[deref]
    #[deref_mut]
    pub(crate) layout: Arc<WrappedLineLayout>,
    pub text: SharedString,
    pub(crate) decoration_runs: SmallVec<[DecorationRun; 32]>,
}

impl WrappedLine {
    pub fn len(&self) -> usize {
        self.layout.len()
    }

    pub fn paint(
        &self,
        origin: Point<Pixels>,
        line_height: Pixels,
        cx: &mut WindowContext,
    ) -> Result<()> {
        paint_line(
            origin,
            &self.layout.unwrapped_layout,
            line_height,
            &self.decoration_runs,
            self.wrap_width,
            &self.wrap_boundaries,
            cx,
        )?;

        Ok(())
    }
}

fn paint_line(
    origin: Point<Pixels>,
    layout: &LineLayout,
    line_height: Pixels,
    decoration_runs: &[DecorationRun],
    wrap_width: Option<Pixels>,
    wrap_boundaries: &[WrapBoundary],
    cx: &mut WindowContext<'_>,
) -> Result<()> {
    let padding_top = (line_height - layout.ascent - layout.descent) / 2.;
    let baseline_offset = point(px(0.), padding_top + layout.ascent);
    let mut decoration_runs = decoration_runs.iter();
    let mut wraps = wrap_boundaries.iter().peekable();
    let mut run_end = 0;
    let mut color = black();
    let mut current_underline: Option<(Point<Pixels>, UnderlineStyle)> = None;
    let text_system = cx.text_system().clone();
    let mut glyph_origin = origin;
    let mut prev_glyph_position = Point::default();
    for (run_ix, run) in layout.runs.iter().enumerate() {
        let max_glyph_size = text_system
            .bounding_box(run.font_id, layout.font_size)?
            .size;

        for (glyph_ix, glyph) in run.glyphs.iter().enumerate() {
            glyph_origin.x += glyph.position.x - prev_glyph_position.x;

            if wraps.peek() == Some(&&WrapBoundary { run_ix, glyph_ix }) {
                wraps.next();
                if let Some((underline_origin, underline_style)) = current_underline.take() {
                    cx.paint_underline(
                        underline_origin,
                        glyph_origin.x - underline_origin.x,
                        &underline_style,
                    )?;
                }

                glyph_origin.x = origin.x;
                glyph_origin.y += line_height;
            }
            prev_glyph_position = glyph.position;

            let mut finished_underline: Option<(Point<Pixels>, UnderlineStyle)> = None;
            if glyph.index >= run_end {
                if let Some(style_run) = decoration_runs.next() {
                    if let Some((_, underline_style)) = &mut current_underline {
                        if style_run.underline.as_ref() != Some(underline_style) {
                            finished_underline = current_underline.take();
                        }
                    }
                    if let Some(run_underline) = style_run.underline.as_ref() {
                        current_underline.get_or_insert((
                            point(
                                glyph_origin.x,
                                origin.y + baseline_offset.y + (layout.descent * 0.618),
                            ),
                            UnderlineStyle {
                                color: Some(run_underline.color.unwrap_or(style_run.color)),
                                thickness: run_underline.thickness,
                                wavy: run_underline.wavy,
                            },
                        ));
                    }

                    run_end += style_run.len as usize;
                    color = style_run.color;
                } else {
                    run_end = layout.len;
                    finished_underline = current_underline.take();
                }
            }

            if let Some((underline_origin, underline_style)) = finished_underline {
                cx.paint_underline(
                    underline_origin,
                    glyph_origin.x - underline_origin.x,
                    &underline_style,
                )?;
            }

            let max_glyph_bounds = Bounds {
                origin: glyph_origin,
                size: max_glyph_size,
            };

            let content_mask = cx.content_mask();
            if max_glyph_bounds.intersects(&content_mask.bounds) {
                if glyph.is_emoji {
                    cx.paint_emoji(
                        glyph_origin + baseline_offset,
                        run.font_id,
                        glyph.id,
                        layout.font_size,
                    )?;
                } else {
                    cx.paint_glyph(
                        glyph_origin + baseline_offset,
                        run.font_id,
                        glyph.id,
                        layout.font_size,
                        color,
                    )?;
                }
            }
        }
    }

    if let Some((underline_start, underline_style)) = current_underline.take() {
        let line_end_x = origin.x + wrap_width.unwrap_or(Pixels::MAX).min(layout.width);
        cx.paint_underline(
            underline_start,
            line_end_x - underline_start.x,
            &underline_style,
        )?;
    }

    Ok(())
}