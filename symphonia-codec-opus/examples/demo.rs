use std::path::PathBuf;

use clap::{builder::TypedValueParser, value_parser, Arg, ArgAction, Command};

use symphonia_codec_opus::dec::OpusDecoder;

fn cmd() -> Command {
    Command::new("demo")
        .version("1.0")
        .about("OPUS demo")
        .arg(
            Arg::new("decode-only")
                .short('d')
                .help("only runs the decoder (reads the bit-stream as input)")
                .action(ArgAction::SetTrue)
                .conflicts_with("encode-only"),
        )
        .arg(
            Arg::new("encode-only")
                .short('e')
                .help("only runs the encoder (output the bit-stream)")
                .action(ArgAction::SetTrue)
                .conflicts_with("decode-only"),
        )
        .arg(
            Arg::new("constant-bitrate")
                .long("cbr")
                .help("enable constant bitrate")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("constrained-variable-bitrate")
                .long("cvbr")
                .help("enable constrained variable bitrate")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("delay-decision")
                .long("delay-decision")
                .help("use look-ahead for speech/music detection (experts only)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("bandwidth")
                .long("bandwidth")
                .help("audio bandwidth")
                .num_args(1)
                .value_parser(["NB", "MB", "WB", "SWB", "FB"]),
        )
        .arg(
            Arg::new("framesize")
                .long("framesize")
                .help("frame size in ms")
                .num_args(1)
                .value_parser(["2.5", "5", "10", "20", "40", "60", "80", "100", "120"])
                .default_value("20"),
        )
        .arg(
            Arg::new("max-payload")
                .long("max-payload")
                .help("maximum payload size in bytes")
                .num_args(1)
                .value_parser(value_parser!(usize))
                .default_value("1024"),
        )
        .arg(
            Arg::new("encoder-complexity")
                .long("complexity")
                .help("encoder complexity, 0 (lowest) ... 10 (highest)")
                .num_args(1)
                .value_parser(value_parser!(u8).range(0..=10))
                .default_value("10"),
        )
        .arg(
            Arg::new("decoder-complexity")
                .long("dec-complexity")
                .help("decoder complexity, 0 (lowest) ... 10 (highest)")
                .num_args(1)
                .value_parser(value_parser!(u8).range(0..=10))
                .default_value("0"),
        )
        .arg(
            Arg::new("inbandfec")
                .long("inbandfec")
                .help("enable SILK inband FEC")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("force-mono")
                .long("forcemono")
                .help("force mono encoding, even for stereo input")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("dtx")
                .long("dtx")
                .help("enable SILK DTX")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("loss")
                .long("loss")
                .help("optimize for loss percentage and simulate packet loss, in percent (0-100)")
                .value_name("perc")
                .num_args(1)
                .value_parser(value_parser!(u8).range(0..=100))
                .default_value("0"),
        )
        .arg(
            Arg::new("loss-file")
                .long("lossfile")
                .help("simulate packet loss, reading loss from file")
                .value_name("file")
                .num_args(1)
                .value_parser(value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("dred")
                .long("dred")
                .help("add Deep REDundancy (in units of 10-ms frames)")
                .value_name("frames")
                .num_args(1)
                .value_parser(value_parser!(usize)),
        )
        .arg(
            Arg::new("samplerate")
                .help("sampling rate (Hz)")
                .num_args(1)
                .required(true),
        )
        .arg(
            Arg::new("channels")
                .help("channels")
                .num_args(1)
                .value_parser(value_parser!(u8).range(0..=1))
                .required(true),
        )
        .arg(
            Arg::new("input")
                .help("input")
                .num_args(1)
                .value_parser(value_parser!(PathBuf))
                .required(true),
        )
        .arg(
            Arg::new("output")
                .help("output")
                .num_args(1)
                .value_parser(value_parser!(PathBuf))
                .required(true),
        );
}

fn decode() {
    let mut decoder = OpusDecoder::new(fs, channels);
}

fn main() {
    let matches = cmd().get_matches();
    if let Some(sr) = matches.get_one::<u32>("samplerate") {}
}
