use image_hasher::{HashAlg, HasherConfig};

pub fn photos_are_similar(img1_bytes: &[u8], img2_bytes: &[u8], threshold: u32) -> bool {
    let hasher = HasherConfig::new().hash_alg(HashAlg::Gradient).to_hasher();

    let img1 = match image::load_from_memory(img1_bytes) {
        Ok(img) => img,
        Err(_) => return false,
    };
    let img2 = match image::load_from_memory(img2_bytes) {
        Ok(img) => img,
        Err(_) => return false,
    };

    let h1 = hasher.hash_image(&img1);
    let h2 = hasher.hash_image(&img2);

    let distance = h1.dist(&h2);
    distance <= threshold
}
