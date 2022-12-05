use exif::experimental::Writer;
use exif::*;
use img_parts::jpeg::Jpeg;
use img_parts::ImageEXIF;
use std::ffi::OsStr;
use std::fs::{read, OpenOptions};
use std::path::{Path, PathBuf};
use xmp::orientation::*;

pub const ALLOWED_EXTENSIONS_RAW: [&str; 37] = [
    "nef", "3fr", "ari", "arw", "bay", "crw", "cr2", "cr3", "cap", "dcs", "dcr", "dng", "drf",
    "eip", "erf", "fff", "gpr", "mdc", "mef", "mos", "mrw", "nrw", "obm", "orf", "pef", "ptx",
    "pxn", "r3d", "raw", "rwl", "rw2", "rwz", "sr2", "srf", "srw", "x3f", "raf",
];
pub const ALLOWED_EXTENSIONS_JPEG: [&str; 3] = ["jpg", "jpeg", "png"];

pub enum Direction {
    Left,
    Right,
}

enum FileType {
    Raw,
    NotRaw,
}
fn create_xml() -> &'static str {
    r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
    <rdf:RDF xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:lr="http://ns.adobe.com/lightroom/1.0/" xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:xmp="http://ns.adobe.com/xap/1.0/">
        <rdf:Description xmlns:exif="http://ns.adobe.com/exif/1.0/" xmlns:tiff="http://ns.adobe.com/tiff/1.0/" xmlns:xmp="http://ns.adobe.com/xap/1.0/" tiff:Orientation="3">
            <dc:subject>
                <rdf:Bag>
                </rdf:Bag>
            </dc:subject>
            <lr:hierarchicalSubject>
                <rdf:Bag>
                </rdf:Bag>
            </lr:hierarchicalSubject>
        </rdf:Description>
    </rdf:RDF>
</x:xmpmeta>"#
}

struct BasicFlip(usize);

impl From<usize> for BasicFlip {
    fn from(item: usize) -> Self {
        Self(item)
    }
}

impl std::ops::Add for BasicFlip {
    type Output = u8;
    fn add(self, other: Self) -> u8 {
        const VALUES: [u8; 4] = [3, 8, 1, 6];
        VALUES[(VALUES.iter().position(|&x| x as usize == self.0).unwrap() + other.0) % 4] as u8
    }
}

impl std::ops::Sub for BasicFlip {
    type Output = u8;
    fn sub(self, other: Self) -> u8 {
        const VALUES: [u8; 4] = [3, 8, 1, 6];
        VALUES[(VALUES.iter().position(|&x| x as usize == self.0).unwrap() as i8 - other.0 as i8) as usize % 4] as u8
    }
}

pub fn rotate(direction: Direction, file_path: &Path) -> Result<bool, String> {
    if !file_path.exists() {
        return Err("Incorrect file path.".to_string());
    }
    Ok(write(direction, file_path)?)
}

fn write(direction: Direction, file_path: &Path) -> Result<bool, String> {
    let file_tupe = file_type(&file_path);
    let mut flip = get_flip(&file_tupe, file_path)?;
    match direction {
        Direction::Left => flip = (BasicFlip::from(flip) - BasicFlip::from(1)) as usize,
        Direction::Right => flip = (BasicFlip::from(flip) + BasicFlip::from(1)) as usize,
    };

    Ok(match file_tupe {
        FileType::Raw => write_flip_raw(flip, file_path.to_path_buf())?,

        FileType::NotRaw => write_flip_not_raw(flip, file_path.to_path_buf())?,
    })
}

fn get_flip(file_type: &FileType, file_path: &Path) -> Result<usize, String> {
    Ok(match file_type {
        FileType::Raw => get_flip_raw(&file_path)?,
        FileType::NotRaw => get_flip_value_not_raw(&file_path)?,
    })
}

fn write_flip_raw(flip: usize, mut file_path: PathBuf) -> Result<bool, String> {
    file_path.set_extension("xmp");
    let x = xmp::UpdateResults {
        orientation: Some(flip),
        ..Default::default()
    };
    match x.update(file_path) {
        Ok(_) => Ok(true),
        Err(_) => Err("Could not update the XMP file.".to_string()),
    }
}

fn write_flip_not_raw(flip: usize, file_path: PathBuf) -> Result<bool, String> {
    let image_desc = Field {
        tag: Tag::Orientation,
        ifd_num: In::PRIMARY,
        value: Value::Short(vec![flip as u16]),
    };

    let mut writer = Writer::new();
    let mut buf = std::io::Cursor::new(Vec::new());
    writer.push_field(&image_desc);
    match writer.write(&mut buf, false) {
        Err(_) => return Err("Cannot write buffer".to_string()),
        _ => (),
    }
    let Ok(file) = read(&file_path) else {
        return Err("Cannot read file".to_string());
    };
    let Ok(mut jpeg) = Jpeg::from_bytes(file.clone().into()) else {
        return Err("Not valid jpeg".to_string());
    };
    jpeg.set_exif(Some(bytes::Bytes::from(bytes::Bytes::from(
        buf.into_inner(),
    ))));
    drop(file);
    let Ok(output) = OpenOptions::new()
        .write(true)
        .open(&file_path) else {
            return Err("Cannot read file".to_string());
        };

    match jpeg.encoder().write_to(output) {
        Ok(_) => Ok(true),
        Err(_) => Err("Cannot update jpeg exif".to_string()),
    }
}

fn get_flip_raw(file_path: &Path) -> Result<usize, String> {
    let mut xmp_file = PathBuf::from(file_path);
    xmp_file.set_extension("xmp");
    if !xmp_file.exists() {
        create_xmp(&file_path)?;
    }
    let Ok(xmp_result) = xmp::UpdateResults::load(&xmp_file)  else {
        return Err("Cannot read XMP file".to_string());
    };
    match xmp_result.orientation {
        Some(x) => Ok(x),
        None => Err("Cannot read XMP file".to_string()),
    }
}

fn get_flip_value_not_raw(file_path: &Path) -> Result<usize, String> {
    let file = std::fs::File::open(file_path).unwrap();
    let mut bufreader = std::io::BufReader::new(&file);
    let exifreader = exif::Reader::new();
    let exif = exifreader.read_from_container(&mut bufreader).unwrap();
    match exif.get_field(Tag::Orientation, In::PRIMARY) {
        Some(orientation) => match orientation.value.get_uint(0) {
            Some(v @ 1..=8) => {
                return Ok(v as usize);
            }
            _ => return Err("Orientation value is broken".to_string()),
        },
        None => return Err("Orientation tag is broken".to_string()),
    }
}

fn file_type(file_path: &Path) -> FileType {
    let extension = file_path
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_lowercase);
    let extension = extension.as_deref();
    if ALLOWED_EXTENSIONS_RAW
        .iter()
        .any(|ext| Some(*ext) == extension)
    {
        FileType::Raw
    } else {
        FileType::NotRaw
    }
}

fn create_xmp(file_path: &Path) -> Result<bool, String> {
    let flip_value_from_raw: u8 = match Orientation::from_raw(&file_path) {
        Ok(x) => x.0.into(),
        _ => return Err("Cannot find flip value in given format".to_string()),
    };
    let file = file_path.with_extension("xmp");
    std::fs::write(&file, create_xml()).unwrap();
    let x = xmp::UpdateResults {
        orientation: Some(flip_value_from_raw as usize),
        ..Default::default()
    };
    x.update(file).unwrap();
    Ok(true)
}
