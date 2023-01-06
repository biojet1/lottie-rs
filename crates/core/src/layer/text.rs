use fontkit::{Area, Line, PathSegment, Span};
use lottie_model::*;

use crate::font::FontDB;
use crate::prelude::RenderableContent;
use crate::Error;

struct GlyphData {
    c: char,
    beziers: Vec<Bezier>,
    offset_x: f32,
}

impl RenderableContent {
    pub fn from_text(
        text: &TextAnimationData,
        model: &Model,
        fontdb: &FontDB,
    ) -> Result<Self, Error> {
        let mut glyph_layers = vec![];
        for keyframe in &text.document.keyframes {
            let parser = TextDocumentParser::new(keyframe, &model, fontdb)?;
            glyph_layers.push(parser.shape_layer()?);
        }

        Ok(RenderableContent::Shape(ShapeGroup {
            shapes: glyph_layers,
        }))
    }
}

#[derive(Clone)]
struct Styles {
    fill: Rgb,
    fill_opacity: f32,
}

struct TextDocumentParser<'a> {
    model: &'a Model,
    fontdb: &'a FontDB,
    area: Area<Styles>,
    lottie_font: &'a Font,
    keyframe: &'a KeyFrame<TextDocument>,
}

impl<'a> TextDocumentParser<'a> {
    fn new(
        keyframe: &'a KeyFrame<TextDocument>,
        model: &'a Model,
        fontdb: &'a FontDB,
    ) -> Result<Self, Error> {
        let doc = &keyframe.start_value;
        let lottie_font = model
            .font(&doc.font_name)
            .ok_or_else(|| Error::FontFamilyNotFound(doc.font_name.clone()))?;
        let font = fontdb
            .font(lottie_font)
            .ok_or_else(|| Error::FontNotLoaded(doc.font_name.clone()))?;
        font.load()?;

        // parse fill/opacity data
        let rgb = Rgb::new_u8(doc.fill_color.r, doc.fill_color.g, doc.fill_color.b);

        let opacity = doc.fill_color.a as f32 / 255.0 * 100.0;
        let styles = Styles {
            fill: rgb,
            fill_opacity: opacity,
        };
        // parse font data
        let metrics = font.measure(&doc.value)?;
        let span = Span {
            font_key: font.key(),
            letter_spacing: 0.0,
            line_height: None,
            size: doc.size,
            broke_from_prev: false,
            metrics,
            swallow_leading_space: false,
            additional: styles,
        };
        let mut area = Area::new();
        let line = Line::new(span);
        area.lines.push(line);

        Ok(TextDocumentParser {
            model,
            fontdb,
            area,
            lottie_font,
            keyframe,
        })
    }

    fn shape_layer(&self) -> Result<ShapeLayer, Error> {
        let font = self
            .fontdb
            .font(self.lottie_font)
            .ok_or_else(|| Error::FontNotLoaded(self.lottie_font.name.clone()))?;
        font.load()?;
        let units = font.units_per_em() as f32;
        let doc = &self.keyframe.start_value;

        let mut result = vec![];
        for line in &self.area.lines {
            for span in &line.spans {
                let factor = span.size / units;
                let mut all_beziers = vec![];
                let mut adv = 0.0;

                // styles

                let fill = self
                    .keyframe
                    .alter_value(span.additional.fill, span.additional.fill);
                let fill_opacity = self
                    .keyframe
                    .alter_value(span.additional.fill_opacity, span.additional.fill_opacity);
                let fill_layer = ShapeLayer {
                    name: None,
                    hidden: false,
                    shape: Shape::Fill(Fill {
                        opacity: Animated {
                            animated: false,
                            keyframes: vec![fill_opacity],
                        },
                        color: Animated {
                            animated: false,
                            keyframes: vec![fill],
                        },
                        fill_rule: FillRule::NonZero,
                    }),
                };
                for c in span.metrics.positions() {
                    let (glyph, _) = font.outline(c.metrics.c).ok_or_else(|| {
                        Error::FontGlyphNotFound(self.lottie_font.name.clone(), c.metrics.c)
                    })?;
                    let mut bezier = Bezier::default();
                    let mut beziers = vec![];
                    let mut last_pt = Vector2D::new(0.0, 0.0);
                    let length = c.metrics.advanced_x as f32 * factor + c.kerning as f32 * factor;
                    for segment in glyph.path.iter() {
                        match segment {
                            PathSegment::MoveTo { x, y } => {
                                if !bezier.verticies.is_empty() {
                                    let mut old = std::mem::replace(&mut bezier, Bezier::default());
                                    old.out_tangent.push(Vector2D::new(0.0, 0.0));
                                    beziers.push(old);
                                }
                                bezier.in_tangent.push(Vector2D::new(0.0, 0.0));
                                last_pt = Vector2D::new(*x as f32, -*y as f32) * factor;
                                bezier.verticies.push(last_pt);
                            }
                            PathSegment::LineTo { x, y } => {
                                let pt = Vector2D::new(*x as f32, -*y as f32) * factor;
                                bezier.out_tangent.push(Vector2D::new(0.0, 0.0));
                                bezier.in_tangent.push(Vector2D::new(0.0, 0.0));
                                bezier.verticies.push(pt);
                                last_pt = pt;
                            }
                            PathSegment::CurveTo {
                                x1,
                                y1,
                                x2,
                                y2,
                                x,
                                y,
                            } => {
                                let pt1 = Vector2D::new(*x1 as f32, -*y1 as f32) * factor;
                                let pt2 = Vector2D::new(*x2 as f32, -*y2 as f32) * factor;
                                let pt = Vector2D::new(*x as f32, -*y as f32) * factor;

                                bezier.out_tangent.push(pt1 - last_pt);
                                bezier.in_tangent.push(pt2 - pt);
                                bezier.verticies.push(pt);
                                last_pt = pt;
                            }
                            PathSegment::ClosePath => {
                                bezier.closed = true;
                            }
                        }
                    }
                    if !bezier.verticies.is_empty() {
                        bezier.out_tangent.push(Vector2D::new(0.0, 0.0));
                        beziers.push(bezier);
                    }
                    all_beziers.push(GlyphData {
                        c: c.metrics.c,
                        beziers,
                        offset_x: adv,
                    });

                    adv += length;
                }

                let glyphs = all_beziers
                    .into_iter()
                    .map(|data| {
                        let GlyphData {
                            c,
                            beziers,
                            offset_x,
                        } = data;

                        let mut transform = Transform::default();
                        transform.position = Some(Animated {
                            animated: false,
                            keyframes: vec![KeyFrame::from_value(Vector2D::new(offset_x, 0.0))],
                        });
                        ShapeLayer {
                            name: Some(format!("{}", c)),
                            hidden: false,
                            shape: Shape::Group {
                                shapes: vec![
                                    ShapeLayer {
                                        name: None,
                                        hidden: false,
                                        shape: Shape::Path {
                                            d: Animated {
                                                animated: false,
                                                keyframes: vec![self
                                                    .keyframe
                                                    .alter_value(beziers.clone(), beziers)],
                                            },
                                        },
                                    },
                                    fill_layer.clone(),
                                    ShapeLayer {
                                        name: None,
                                        hidden: false,
                                        shape: Shape::Transform(transform),
                                    },
                                ],
                            },
                        }
                    })
                    .collect::<Vec<_>>();
                let glyphs = ShapeLayer {
                    name: None,
                    hidden: false,
                    shape: Shape::Group { shapes: glyphs },
                };

                let start_shift_x = match doc.justify {
                    TextJustify::Left => 0.0,
                    TextJustify::Center => -adv / 2.0,
                    TextJustify::Right => -adv,
                    _ => 0.0, // TODO: support other TextJustify options
                };
                let start_shift_y = -doc.baseline_shift;
                let shift = Vector2D::new(start_shift_x, start_shift_y);
                let transform_position = self.keyframe.alter_value(shift, shift);

                let mut transform = Transform::default();
                transform.position = Some(Animated {
                    animated: false,
                    keyframes: vec![transform_position],
                });
                result.push(ShapeLayer {
                    name: None,
                    hidden: false,
                    shape: Shape::Group {
                        shapes: vec![
                            glyphs,
                            ShapeLayer {
                                name: None,
                                hidden: false,
                                shape: Shape::Transform(transform),
                            },
                        ],
                    },
                });
            }
        }
        Ok(ShapeLayer {
            name: None,
            hidden: false,
            shape: Shape::Group { shapes: result },
        })
    }
}
