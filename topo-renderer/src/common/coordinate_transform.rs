use thiserror::Error;
use tiff::decoder::DecodingResult;

#[derive(Error, Debug)]
pub enum CoordinateTransformError {
    #[error(
        "Incorrect geo tags: only ModelPixelScaleTag and ModelTiepointTag without ModelTransformationTag supported"
    )]
    IncorrectGeoTags,
    #[error(
        "Incorrect geo tag data: ModelPixelScaleTag should have 3 and ModelTiepointTag should have 6 values"
    )]
    IncorrectGeoTagData,
}

pub struct CoordinateTransform {
    pub raster_point: (f32, f32),
    pub model_point: (f32, f32),
    pub pixel_scale: (f32, f32),
}

impl CoordinateTransform {
    pub fn from_geo_tag_data(
        pixel_scale_data: Option<Vec<f64>>,
        tie_points_data: Option<Vec<f64>>,
        model_transformation_data: Option<Vec<f64>>,
    ) -> Result<Self, CoordinateTransformError> {
        if model_transformation_data.is_some() {
            return Err(CoordinateTransformError::IncorrectGeoTags);
        }
        if let Some(pixel_scale_data) = pixel_scale_data
            && let Some(tie_points_data) = tie_points_data
        {
            if let &[pixel_scale_x, pixel_scale_y, _] = pixel_scale_data.as_slice()
                && let &[
                    raster_point_x,
                    raster_point_y,
                    _,
                    model_point_x,
                    model_point_y,
                    _,
                ] = tie_points_data.as_slice()
            {
                Ok(Self {
                    raster_point: (raster_point_x as f32, raster_point_y as f32),
                    model_point: (model_point_x as f32, model_point_y as f32),
                    pixel_scale: (pixel_scale_x as f32, pixel_scale_y as f32),
                })
            } else {
                Err(CoordinateTransformError::IncorrectGeoTagData)
            }
        } else {
            Err(CoordinateTransformError::IncorrectGeoTags)
        }
    }

    pub fn to_model(&self, coord: (f32, f32)) -> (f32, f32) {
        (
            (coord.0 - self.raster_point.0) * self.pixel_scale.0 + self.model_point.0,
            (coord.1 - self.raster_point.1) * -self.pixel_scale.1 + self.model_point.1,
        )
    }

    pub fn to_raster(&self, coord: (f32, f32)) -> (f32, f32) {
        (
            (coord.0 - self.model_point.0) / self.pixel_scale.0 + self.raster_point.0,
            (coord.1 - self.model_point.1) / -self.pixel_scale.1 + self.raster_point.1,
        )
    }
}

pub fn get_height_value_at(
    height_map_decoding_result: &DecodingResult,
    coordinate_transform: &CoordinateTransform,
    size: (u32, u32),
    longitude: f64,
    latitude: f64,
) -> Option<f32> {
    let raster = coordinate_transform.to_raster((longitude as f32, latitude as f32));
    let index = raster.1 as usize * size.0 as usize + raster.0 as usize;
    match height_map_decoding_result {
        DecodingResult::F32(vec) => vec.get(index).copied(),
        DecodingResult::F64(vec) => vec.get(index).copied().map(|x| x as f32),
        _ => None,
    }
}
