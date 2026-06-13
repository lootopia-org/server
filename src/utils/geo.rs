const EARTH_RADIUS_METERS: f64 = 6_371_000.0;

fn parse_coord(value: &str) -> Option<f64> {
    value.trim().parse().ok()
}

pub fn distance_meters(lat1: &str, lng1: &str, lat2: &str, lng2: &str) -> Option<f64> {
    let lat1 = parse_coord(lat1)?;
    let lng1 = parse_coord(lng1)?;
    let lat2 = parse_coord(lat2)?;
    let lng2 = parse_coord(lng2)?;

    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();
    let delta_lat = (lat2 - lat1).to_radians();
    let delta_lng = (lng2 - lng1).to_radians();

    let a = (delta_lat / 2.0).sin().powi(2)
        + lat1_rad.cos() * lat2_rad.cos() * (delta_lng / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

    Some(EARTH_RADIUS_METERS * c)
}

pub fn within_proximity(
    lat1: &str,
    lng1: &str,
    lat2: &str,
    lng2: &str,
    threshold_meters: f64,
) -> bool {
    distance_meters(lat1, lng1, lat2, lng2)
        .map(|d| d <= threshold_meters)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::distance_meters;

    #[test]
    fn same_point_is_zero_distance() {
        let distance = distance_meters("51.5074", "-0.1278", "51.5074", "-0.1278").unwrap();
        assert!(distance.abs() < 0.01);
    }

    #[test]
    fn nearby_points_are_within_threshold() {
        let distance = distance_meters("51.5074", "-0.1278", "51.5080", "-0.1278").unwrap();
        assert!(distance > 0.0);
        assert!(distance < 100.0);
    }
}
