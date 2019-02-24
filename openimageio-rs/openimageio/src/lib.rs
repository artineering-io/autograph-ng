use std::ffi::CStr;
use std::os::raw::c_char;

mod attribute;
mod buffer;
mod cache;
mod error;
mod input;
mod output;
mod roi;
mod spec;
mod typedesc;

pub use buffer::ImageBuffer;
pub use error::Error;
pub use input::ImageInput;
pub use output::ImageOutput;
pub use output::MultiImageOutput;
pub use output::SingleImageOutput;
pub use spec::AllChannels;
pub use spec::Channel;
pub use spec::ChannelAlpha;
pub use spec::ChannelDesc;
pub use spec::ChannelRGB;
pub use spec::ChannelRGBA;
pub use spec::ImageSpec;
pub use spec::ImageSpecOwned;
pub use typedesc::Aggregate;
pub use typedesc::BaseType;
pub use typedesc::TypeDesc;
pub use typedesc::VecSemantics;

pub use cache::ImageCache;

unsafe fn cstring_to_owned(cstr: *const c_char) -> String {
    // assume utf8 input
    let msg = CStr::from_ptr(cstr).to_str().unwrap().to_owned();
    openimageio_sys::OIIO_freeString(cstr);
    msg
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;
    use std::slice;

    #[test]
    fn open_image() {
        let img = ImageInput::open("../test_images/tonberry.jpg");
        assert!(img.is_ok());
    }

    /// test all API forms
    #[test]
    fn test_api() {
        let mut img = ImageInput::open("../test_images/kazeharu.png").unwrap();

        // members on imageinput
        img.spec();
        img.width();
        img.height();
        img.depth();
        img.read::<f32>().unwrap();
        img.all_channels();
        img.channels(&[0]).unwrap();
        img.channels_by_name(&["R"]).unwrap();
        img.channels_rgba().unwrap();
        img.channel_alpha().unwrap();
        img.all_channels().read::<f32>().unwrap();

        // members on subimage
        let sub = img.subimage(0).unwrap();
        sub.spec();
        sub.width();
        sub.height();
        sub.depth();
        sub.read::<f32>().unwrap(); // consumes
        img.subimage(0).unwrap().spec();
        img.subimage(0).unwrap().width();
        img.subimage(0).unwrap().height();
        img.subimage(0).unwrap().depth();
        img.subimage(0).unwrap().all_channels();
        img.subimage(0).unwrap().channels(&[0]).unwrap();
        img.subimage(0).unwrap().channels_by_name(&["R"]).unwrap();
        img.subimage(0).unwrap().channels_rgba().unwrap();
        img.subimage(0).unwrap().channel_alpha().unwrap();
        img.subimage(0).unwrap().read::<f32>().unwrap();
        img.subimage(0)
            .unwrap()
            .all_channels()
            .read::<f32>()
            .unwrap();

        // members on subimage (same as above, but through subimage+mipmap)
        let sub = img.subimage_mipmap(0, 0).unwrap();
        sub.spec();
        sub.width();
        sub.height();
        sub.depth();
        sub.read::<f32>().unwrap(); // consumes
        img.subimage_mipmap(0, 0).unwrap().spec();
        img.subimage_mipmap(0, 0).unwrap().width();
        img.subimage_mipmap(0, 0).unwrap().height();
        img.subimage_mipmap(0, 0).unwrap().depth();
        img.subimage_mipmap(0, 0).unwrap().all_channels();
        img.subimage_mipmap(0, 0).unwrap().channels(&[0]).unwrap();
        img.subimage_mipmap(0, 0)
            .unwrap()
            .channels_by_name(&["R"])
            .unwrap();
        img.subimage_mipmap(0, 0).unwrap().channels_rgba().unwrap();
        img.subimage_mipmap(0, 0).unwrap().channel_alpha().unwrap();
        img.subimage_mipmap(0, 0).unwrap().read::<f32>().unwrap();
        img.subimage_mipmap(0, 0)
            .unwrap()
            .all_channels()
            .read::<f32>()
            .unwrap();

        // members of SubimageMipmapChannels
        let chan = img.subimage_mipmap(0, 0).unwrap().all_channels();
        chan.spec();
        chan.width();
        chan.height();
        chan.depth();
        chan.read::<f32>().unwrap(); // consumes
        img.subimage_mipmap(0, 0).unwrap().all_channels().spec();
        img.subimage_mipmap(0, 0).unwrap().all_channels().width();
        img.subimage_mipmap(0, 0).unwrap().all_channels().height();
        img.subimage_mipmap(0, 0).unwrap().all_channels().depth();
        img.subimage_mipmap(0, 0)
            .unwrap()
            .all_channels()
            .read::<f32>()
            .unwrap();
    }

    #[test]
    fn open_image_exr() {
        let mut img = ImageInput::open("../test_images/output0013.exr").unwrap();

        for ch in img.spec().channels() {
            //println!("channel {:?}", ch);
        }

        let chans: Vec<_> = img
            .spec()
            .find_channels(r"RenderLayer\.DiffCol\..*")
            .map(|c| c.0)
            .collect();
        //println!("selected channels {:?}", chans);
        let size = (img.width(), img.height());
        let data: ImageBuffer<f32> = img.channels(&chans).unwrap().read().unwrap();
        let spec = ImageSpecOwned::new_2d(TypeDesc::FLOAT, size.0, size.1, &["R", "G", "B"]);
        let mut out = ImageOutput::create("output.exr").unwrap();
        let mut out = out.open(&spec).unwrap();
        out.write_image(data.data()).unwrap();
    }

    #[test]
    fn open_image_psd() {
        let mut img = ImageInput::open("../test_images/cup.psd").unwrap();
        for ch in img.spec().channels() {
            println!("channel {:?}", ch);
        }
    }

    #[test]
    fn open_image_tif() {
        let mut img = ImageInput::open("../test_images/cup.tif").unwrap();
        for ch in img.spec().channels() {
            println!("channel {:?}", ch);
        }
    }

    #[test]
    fn open_nonexistent_image() {
        let img = ImageInput::open("../test_images/nonexistent.png");
        if let Err(ref e) = img {
            println!("{}", e);
        }
        assert!(img.is_err());
    }

    #[test]
    fn test_cache_api() {
        let cache = ImageCache::new();

        cache.image("../test_images/cup.tif").unwrap();
        cache.image("../test_images/cup.tif").unwrap();
        cache.image("../test_images/cup.psd").unwrap();
        cache.image("../test_images/output0013.exr").unwrap();
        cache.image("../test_images/tonberry.jpg").unwrap();
        let img = cache.image("../test_images/kazeharu.png").unwrap();

        // members on CachedImage
        img.spec();
        img.width();
        img.height();
        img.depth();
        img.clone().read::<f32>().unwrap();
        img.clone().all_channels();
        img.clone().channels(&[0]).unwrap();
        img.clone().channels_by_name(&["R"]).unwrap();
        img.clone().channels_rgba().unwrap();
        img.clone().channel_alpha().unwrap();
        img.clone().all_channels().read::<f32>().unwrap();

        // members on CachedSubimageMipmap
        let sub = img.clone().subimage(0).unwrap();
        sub.spec();
        sub.width();
        sub.height();
        sub.depth();
        sub.read::<f32>().unwrap(); // consumes
        cache.image("../test_images/kazeharu.png").unwrap().subimage(0).unwrap().spec();
        cache.image("../test_images/kazeharu.png").unwrap().subimage(0).unwrap().width();
        cache.image("../test_images/kazeharu.png").unwrap().subimage(0).unwrap().height();
        cache.image("../test_images/kazeharu.png").unwrap().subimage(0).unwrap().depth();
        cache.image("../test_images/kazeharu.png").unwrap().subimage(0).unwrap().all_channels();
        cache.image("../test_images/kazeharu.png").unwrap().subimage(0).unwrap().channels(&[0]).unwrap();
        cache.image("../test_images/kazeharu.png").unwrap().subimage(0).unwrap().channels_by_name(&["R"]).unwrap();
        cache.image("../test_images/kazeharu.png").unwrap().subimage(0).unwrap().channels_rgba().unwrap();
        cache.image("../test_images/kazeharu.png").unwrap().subimage(0).unwrap().channel_alpha().unwrap();
        cache.image("../test_images/kazeharu.png").unwrap().subimage(0).unwrap().read::<f32>().unwrap();
        cache.image("../test_images/kazeharu.png").unwrap().subimage(0)
            .unwrap()
            .all_channels()
            .read::<f32>()
            .unwrap();

        // members on CachedSubimageMipmap (same as above, but through subimage+mipmap)
        let sub = img.clone().subimage_mipmap(0, 0).unwrap();
        sub.spec();
        sub.width();
        sub.height();
        sub.depth();
        sub.read::<f32>().unwrap(); // consumes
        cache.image("../test_images/kazeharu.png").unwrap().subimage_mipmap(0, 0).unwrap().spec();
        cache.image("../test_images/kazeharu.png").unwrap().subimage_mipmap(0, 0).unwrap().width();
        cache.image("../test_images/kazeharu.png").unwrap().subimage_mipmap(0, 0).unwrap().height();
        cache.image("../test_images/kazeharu.png").unwrap().subimage_mipmap(0, 0).unwrap().depth();
        cache.image("../test_images/kazeharu.png").unwrap().subimage_mipmap(0, 0).unwrap().all_channels();
        cache.image("../test_images/kazeharu.png").unwrap().subimage_mipmap(0, 0).unwrap().channels(&[0]).unwrap();
        cache.image("../test_images/kazeharu.png").unwrap().subimage_mipmap(0, 0).unwrap().channels_by_name(&["R"]).unwrap();
        cache.image("../test_images/kazeharu.png").unwrap().subimage_mipmap(0, 0).unwrap().channels_rgba().unwrap();
        cache.image("../test_images/kazeharu.png").unwrap().subimage_mipmap(0, 0).unwrap().channel_alpha().unwrap();
        cache.image("../test_images/kazeharu.png").unwrap().subimage_mipmap(0, 0).unwrap().read::<f32>().unwrap();
        cache.image("../test_images/kazeharu.png").unwrap().subimage_mipmap(0, 0).unwrap().all_channels().read::<f32>().unwrap();

        // members of CachedSubimageMipmapChannels
        let chan = img.subimage_mipmap(0, 0).unwrap().all_channels();
        chan.spec();
        chan.width();
        chan.height();
        chan.depth();
        chan.read::<f32>().unwrap(); // consumes.
        cache.image("../test_images/kazeharu.png").unwrap().subimage_mipmap(0, 0).unwrap().all_channels().spec();
        cache.image("../test_images/kazeharu.png").unwrap().subimage_mipmap(0, 0).unwrap().all_channels().width();
        cache.image("../test_images/kazeharu.png").unwrap().subimage_mipmap(0, 0).unwrap().all_channels().height();
        cache.image("../test_images/kazeharu.png").unwrap().subimage_mipmap(0, 0).unwrap().all_channels().depth();
        cache.image("../test_images/kazeharu.png").unwrap().subimage_mipmap(0, 0)
            .unwrap()
            .all_channels()
            .read::<f32>()
            .unwrap();
    }

}
