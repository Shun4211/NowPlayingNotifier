//! WIC による画像のデコード（アルバムアート用）。

use windows::core::Result;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};

/// 乗算済みアルファの BGRA 画像。
#[derive(Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// 画像ファイルのバイト列をデコードする。幅が `max_width` を超える場合は縮小する。
pub fn decode_image(bytes: &[u8], max_width: u32) -> Result<Image> {
    unsafe {
        // ワーカースレッドから呼ばれるので COM を初期化しておく（既に初期化済みでも問題ない）
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let stream = factory.CreateStream()?;
        stream.InitializeFromMemory(bytes)?;
        let decoder = factory.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)?;
        let frame = decoder.GetFrame(0)?;

        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h)?;
        if w == 0 || h == 0 {
            return Err(windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL));
        }

        let source: IWICBitmapSource = if w > max_width {
            let new_h = ((h as u64 * max_width as u64) / w as u64).max(1) as u32;
            let scaler = factory.CreateBitmapScaler()?;
            scaler.Initialize(&frame, max_width, new_h, WICBitmapInterpolationModeHighQualityCubic)?;
            w = max_width;
            h = new_h;
            scaler.into()
        } else {
            frame.into()
        };

        let converter = factory.CreateFormatConverter()?;
        converter.Initialize(
            &source,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )?;

        let stride = w * 4;
        let mut pixels = vec![0u8; (stride * h) as usize];
        converter.CopyPixels(std::ptr::null(), stride, &mut pixels)?;
        Ok(Image { width: w, height: h, pixels })
    }
}
