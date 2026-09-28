# Motion-sensitive spectral rim

The compositor separates Gaussian-filtered silhouette coverage into red outer
and green inner fringes. These are emitted RGB added after tone mapping; they do
not increase opacity or darken the desktop. Face RGB is never sampled by this
effect. A single whole-body silhouette keeps the contour attached to deformation.

Screen-space velocity is exponentially filtered (180 ms). The wake direction
combines opposite velocity with existing body lag, with its Y axis converted
from local-body to screen coordinates. Width is 1 native pixel at rest and up to
9.2 pixels at peak motion on the trailing edge; the leading edge stays thinner.
The normal comes from the existing Gaussian silhouette. Seventeen subpixel
Gaussian taps per spectral field avoid discrete contour echoes at larger widths.
Flat empty/interior regions skip the spectral work. No new render targets or
changes to physics, eye material, or hit testing are introduced.

Reference: Unity's [chromatic aberration documentation](https://docs.unity.cn/Packages/com.unity.postprocessing%403.4/manual/Chromatic-Aberration.html)
describes channel fringing and spectral palettes. This implementation is an
original silhouette-only emission variant, not a copied fullscreen distortion.

Validation uses production GPU captures at rest and for right/left/down velocity
on black, white and busy backgrounds, plus WGSL validation and native paired
release build. Additive light is naturally less visible on already-white pixels.
