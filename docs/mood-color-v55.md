# Mood color and joy aura

Body tint now follows the presented smile/frown, brow tension/raise and affective arousal, stress and valence. The deliberately restrained palette uses warm rose/peach for delight, cool pearl for calm, blue-violet for concern, mint for attentive interest and a muted rose under tension. These are stylized communication conventions, not diagnoses of emotion. Identity pigment and saved organism state are unchanged.

Tint and glow use cascaded low-pass stages with bounded internal time steps. Glow requires a sufficiently positive smile and is inhibited by tension; it fades more slowly than it rises. No free-running hue cycle or random flash is used. A material-space blend between current and delayed tint creates a broad transition across the liquid body and settles when the state settles. It does not follow the face or change the surface geometry.

The joy aura uses the existing blurred body silhouette, increases its spread and alpha smoothly, stays behind the contrast shadow, and uses the mood tint. Presentation padding grows with the aura; collision size is unchanged.

Validation includes bounded onset/recovery and 30/120 Hz equivalence, WGSL uniform-layout validation, Clippy, and production captures of calm, delight onset, full delight, contentment, concern, tension and curiosity on three backgrounds. Supplied-state captures verify appearance; they do not establish frequency in autonomous life.
