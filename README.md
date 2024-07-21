# Symphonia-VOIP codec/format collections

I've been working with Rust for a while, howerver, I'm just a newbie learning voip stuff, so these codes could be crap or whatever. There are still some concepts seems a little fuzzy in my head, so, what the hell, let's just go for it.

This repo contains some voip codecs and their associating storage formats. Currently targeting codec and formats are list below:

codec:
  - G.711 PCMA (Pulse code modulation A-law/symphonia-codec-pcm)
  - G.711 PCMU (Pulse code modulation μ-law/symphonia-codec-pcm)
  - G.722
  - G.722.1
  - EVS (Enhanced Voice Service)
  - AMR (Adaptive Multi-Rate)
  - AMRWB (Adaptive Multi-Rate Wideband)

format:
  - EVS storage format (3gpp TS26.445)
  - AMRWB storage format (RFC 4867)
  - rtpdump format (rtptool, unsupported yet)

VOIP codec algorithms are way too complex. It would take quite a lot of time to rewrite even just one, and the benifit seems to be insignificant, probably. So the best way to obtain decode ability of these codec for Rust is to use existing ANSI-C code to write a thin wrapper. Just like how ffmpeg does(I've never read ffmpeg source code, it's just my guess).

Maybe in the future there would be pure Rust codecs, I would like to replace current C-FFI with that.

## Safety

So, safety huh?

Let's just face it, these codes are definitely **NOT SAFE**, use them on your own risk.

I will try to write unsafe codes as **LESS** as possible, but since we are interfacing with c code, unsafe is needed.

## voip-replay

voip-replay is almost the same to symphonia-play, except that it has voip related format and codec registered.