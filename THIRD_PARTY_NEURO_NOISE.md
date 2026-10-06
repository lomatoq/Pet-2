# Neuro Noise attribution

The folded sine-feedback field in `crates/pet_body/src/liquid_surface.wgsl`
is adapted from **Neuro Noise** by Ksenia Kondrashova (@ksenia-k), whose
Pen credits @zozuar for the original effect.

- Source: https://codepen.io/ksenia-k/pen/vYwgrWv
- Original credited effect: https://x.com/zozuar/status/1625182758745128981/
- Public Pen licensing: https://blog.codepen.io/documentation/licensing/

The adaptation uses WGSL, ten softened and tapered folds, bounded radiance,
body-local and masked facial coordinates, integrated phases and the pet's
existing emotional palette and agitation. Sine feedback is damped and the
field is intentionally band-limited for a more diffuse appearance.
It does not include the Pen's JavaScript or its high-power amplification.

## MIT License

Copyright (c) Ksenia Kondrashova (@ksenia-k)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
of the Software, and to permit persons to whom the Software is furnished to
do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
