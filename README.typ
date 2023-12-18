= Symphonia-VOIP codec/format collections

I've been working with Rust for a while, howerver, I'm just a newbie learning voip stuff, so these codes could be crap or whatever. There are some concepts still seems a little fuzzy in my head, so, what the hell, let's just go for it.

---

This repo contains some voip codecs and their associating storage formats. Currently targeting codec and formats are list below:

codec:
  - EVS (Enhanced Voice Service)
  - AMR (Adaptive Multi-Rate)
  - AMRWB (Adaptive Multi-Rate Wideband)

format:
  - EVS storage format (3gpp TS26.445)
  - AMRWB storage format (RFC 4867)
  - rtpdump format (rtptool)

---

VOIP codec algorithms are way too complex, it would take quite a lot of time to rewrite even just one, and the benifit seems to be quite low, probably. So the best way to obtain decode ability of these codec for Rust is to use existing ANSI-C code to write a thin wrapper. Just like how ffmpeg does.

== Safety

So, safety huh?

Let's just face it, these codes are definitely *NOT SAFE*, use them on your own risk.

I will try to write unsafe codes as less as possible, but since we are interfacing with c code, unsafe is needed.

== voip-replay

voip-replay is almost the same to symphonia-play, except that it has voip related format and codec registered.