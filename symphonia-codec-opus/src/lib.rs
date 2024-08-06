mod celt;
mod silk;

#[derive(Clone, Copy, Debug, Default)]
pub enum BandWidth {
    Narrowband,
    #[default]
    Mediumband,
    Wideband = 40,
    SuperWideband = 60,
}
