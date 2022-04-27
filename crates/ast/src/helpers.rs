use std::fmt;

use super::*;
use serde::de::{Error, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub fn bool_from_int<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    match u8::deserialize(deserializer)? {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(serde::de::Error::invalid_value(
            serde::de::Unexpected::Unsigned(other as u64),
            &"zero or one",
        )),
    }
}

pub fn int_from_bool<S>(b: &bool, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_u8(if *b { 1 } else { 0 })
}

pub fn str_to_rgba<'de, D>(deserializer: D) -> Result<Rgba, D::Error>
where
    D: Deserializer<'de>,
{
    let s = <&str>::deserialize(deserializer)?;
    Ok(s.parse().unwrap())
}

pub fn str_from_rgba<S>(b: &Rgba, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&b.to_string())
}

impl<'de> serde::Deserialize<'de> for LayerContent {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(d)?;

        #[derive(Serialize, Deserialize)]
        struct SolidColor {
            #[serde(
                rename = "sc",
                deserialize_with = "str_to_rgba",
                serialize_with = "str_from_rgba"
            )]
            color: Rgba,
            #[serde(rename = "sh")]
            height: f32,
            #[serde(rename = "sw")]
            width: f32,
        }

        Ok(
            match value.get("ty").and_then(serde_json::Value::as_u64).unwrap() {
                0 => LayerContent::Precomposition(PreCompositionRef::deserialize(value).unwrap()),
                1 => {
                    let color = SolidColor::deserialize(value).unwrap();
                    LayerContent::SolidColor {
                        color: color.color,
                        height: color.height,
                        width: color.width,
                    }
                }
                // 2 => LayerContent::Image(Type2::deserialize(value).unwrap()),
                3 => LayerContent::Empty,
                4 => {
                    let shapes = value
                        .get("shapes")
                        .map(|v| Vec::<ShapeLayer>::deserialize(v))
                        .transpose()
                        .unwrap_or_default()
                        .unwrap_or_default();
                    LayerContent::Shape(ShapeGroup { shapes })
                }
                // 5 => LayerContent::SolidColor(Type1::deserialize(value).unwrap()),
                // 6 => LayerContent::Image(Type2::deserialize(value).unwrap()),
                // 7 => LayerContent::Null(Type3::deserialize(value).unwrap()),
                type_ => panic!("unsupported type {:?}", type_),
            },
        )
    }
}

impl Serialize for LayerContent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        #[serde(untagged)]
        enum LayerContent_<'a> {
            // T1(&'a Type1),
            SolidColor { sc: String, sh: f32, sw: f32 },
            Shape { shapes: &'a Vec<ShapeLayer> },
        }

        #[derive(Serialize)]
        struct TypedLayerContent<'a> {
            #[serde(rename = "ty")]
            t: u64,
            #[serde(flatten)]
            content: LayerContent_<'a>,
        }

        let msg = match self {
            LayerContent::Shape(ShapeGroup { shapes }) => TypedLayerContent {
                t: 4,
                content: LayerContent_::Shape { shapes },
            },
            LayerContent::SolidColor {
                color,
                height,
                width,
            } => TypedLayerContent {
                t: 1,
                content: LayerContent_::SolidColor {
                    sc: color.to_string(),
                    sh: *height,
                    sw: *width,
                },
            },
            _ => unimplemented!(),
        };
        msg.serialize(serializer)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum Value {
    Primitive(f32),
    List(Vec<f32>),
    ComplexBezier(Vec<Bezier>),
}

impl Value {
    fn as_f32_vec(&self) -> Option<Vec<f32>> {
        Some(match self {
            Value::Primitive(p) => vec![*p],
            Value::List(l) => l.clone(),
            _ => return None,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum AnimatedHelper {
    Plain(Value),
    AnimatedHelper(Vec<KeyFrame<Value>>),
}

impl<'a, T> From<&'a Vec<KeyFrame<T>>> for AnimatedHelper {
    fn from(_: &'a Vec<KeyFrame<T>>) -> Self {
        todo!()
    }
}

impl<T> From<AnimatedHelper> for Vec<KeyFrame<T>>
where
    T: FromTo<Value>,
{
    fn from(animated: AnimatedHelper) -> Self {
        match animated {
            AnimatedHelper::Plain(v) => {
                vec![KeyFrame {
                    value: T::from(v),
                    start_frame: None,
                    easing_in: None,
                    easing_out: None,
                }]
            }
            AnimatedHelper::AnimatedHelper(v) => v
                .into_iter()
                .map(|keyframe| KeyFrame {
                    value: T::from(keyframe.value),
                    start_frame: keyframe.start_frame,
                    easing_in: keyframe.easing_in,
                    easing_out: keyframe.easing_out,
                })
                .collect(),
        }
    }
}

pub(crate) trait FromTo<T> {
    fn from(v: T) -> Self;
    fn to(self) -> T;
}

impl FromTo<Value> for Vector2D {
    fn from(v: Value) -> Self {
        let v = v.as_f32_vec().unwrap();
        Vector2D::new(v[0], v[1])
    }

    fn to(self) -> Value {
        todo!()
    }
}

impl FromTo<Value> for f32 {
    fn from(v: Value) -> Self {
        let v = v.as_f32_vec().unwrap();
        v[0]
    }

    fn to(self) -> Value {
        Value::Primitive(self)
    }
}

impl FromTo<Value> for Rgb {
    fn from(v: Value) -> Self {
        let v = v.as_f32_vec().unwrap();
        Rgb::new_f32(v[0], v[1], v[2])
    }

    fn to(self) -> Value {
        Value::List(vec![
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
        ])
    }
}

impl FromTo<Value> for Vec<Bezier> {
    fn from(v: Value) -> Self {
        match v {
            Value::ComplexBezier(b) => b,
            _ => todo!(),
        }
    }

    fn to(self) -> Value {
        Value::ComplexBezier(self)
    }
}

pub(crate) fn keyframes_from_array<'de, D, T>(deserializer: D) -> Result<Vec<KeyFrame<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: FromTo<Value>,
{
    let result = AnimatedHelper::deserialize(deserializer).unwrap();
    Ok(result.into())
}

pub fn array_from_keyframes<S, T>(b: &Vec<KeyFrame<T>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let animated = AnimatedHelper::from(b);
    match animated {
        AnimatedHelper::Plain(data) => data.serialize(serializer),
        AnimatedHelper::AnimatedHelper(data) => {
            let mut seq = serializer.serialize_seq(Some(data.len()))?;
            for keyframe in data {
                seq.serialize_element(&keyframe)?;
            }
            seq.end()
        }
    }
}

impl<'de> Deserialize<'de> for AnimatedColorList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        todo!()
    }
}

impl Serialize for AnimatedColorList {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        todo!()
    }
}

pub fn default_vec2_100() -> Animated<Vector2D> {
    Animated {
        animated: false,
        keyframes: vec![KeyFrame::from_value(Vector2D::new(100.0, 100.0))],
    }
}

pub fn default_number_100() -> Animated<f32> {
    Animated {
        animated: false,
        keyframes: vec![KeyFrame::from_value(100.0)],
    }
}

pub fn u32_from_number<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(deserializer.deserialize_any(NumberVistor)?.unwrap())
}

pub fn optional_u32_from_number<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(NumberVistor)
}

struct NumberVistor;

impl<'de> Visitor<'de> for NumberVistor {
    type Value = Option<u32>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("u32 / f32")
    }

    fn visit_f32<E: Error>(self, v: f32) -> Result<Self::Value, E> {
        Ok(Some(v.round() as u32))
    }

    fn visit_f64<E: Error>(self, v: f64) -> Result<Self::Value, E> {
        Ok(Some(v.round() as u32))
    }

    fn visit_i64<E: Error>(self, v: i64) -> Result<Self::Value, E> {
        Ok(Some(v as u32))
    }

    fn visit_u64<E: Error>(self, v: u64) -> Result<Self::Value, E> {
        Ok(Some(v as u32))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Ok(None)
    }
}

pub(crate) fn vec_from_array<'de, D>(deserializer: D) -> Result<Vec<Vector2D>, D::Error>
where
    D: Deserializer<'de>,
{
    let result = Vec::<[f32; 2]>::deserialize(deserializer)?;
    Ok(result.into_iter().map(|f| f.into()).collect())
}

pub fn array_from_vec<S>(data: &Vec<Vector2D>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut seq = serializer.serialize_seq(Some(data.len()))?;
    for d in data {
        seq.serialize_element(&[d.x, d.y])?;
    }
    seq.end()
}
