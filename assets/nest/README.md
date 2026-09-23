# Pearl cradle and toy, V29 layer correction

Generated with the built-in `image_gen.imagegen` tool. The `nest-pearl` and
`orb-pearl` PNGs are the selected source outputs. RGBA files contain two little-endian u32 dimensions followed by
the exact source RGBA pixels; no painted alterations or alpha extraction were
applied after generation. Transparent corner and exterior pixels were checked.
`manifest.json` records sizes, hashes and alpha coverage.

The cradle is deliberately a softly padded dark lavender capsule with broad
diffuse highlights and a plush pale interior. The toy uses the creature's milky cyan/lavender pearl material
and has no gold star or separate visual theme. Birth capsule artwork and its
choreography are unchanged.

## Aligned depth layers

The nest is rendered in two passes, using the same source and transform:

1. `back`: rear shell and cushion composed behind the pet and toy.
2. `front`: the near shell/rim cutout, composed over content inside the bowl.

The front mask is defined in `crates/pet_body/src/birth_scene.wgsl`. The editable
exports `nest-back-v29.png` and `nest-front-v29.png` use this exact mask and the
same full canvas. `export_layers.py` deterministically compiles the generated
source into these two slices; it does not reframe or paint the artwork. The
original source remains available. The V29 mask follows the actual visible
cushion/shell interface with a shape-preserving cubic through 21 authored
points in `rim-contour-v29.json`. The center is at source y=660; the side
shoulders return down to the silhouette. The prior V28 parabola crossed the
unbroken side shell and produced an unnatural diagonal edge. It is retained
only in the old exported PNGs for comparison.

The slice is a depth boundary, not a collision or admission test. A creature
outside the bowl must be rendered in front of the complete nest. Only content
whose physical state places it inside may be occluded by the front layer.

The split uses `A_front=A_source*mask` and
`A_back=(A_source-A_front)/(1-A_front)`. Source-over composition restores source
coverage exactly before byte quantization; `layer-validation-v29.json` records less
than one 8-bit alpha level of export error. Both layers preserve source RGB
exactly and align without manual positioning. The runtime uses the same formula
directly from the full source, avoiding export quantization.

The physical cushion seat is `0.20 * displayed_nest_width` below the den anchor;
the art base is `0.292 * width` below it. The app and toy settling use that same
seat calculation. Stored toys move to their real world-space resting position
through a critically damped spring. Their grab coordinates remain the rendered
coordinates; the remaining visual cushion settling is under half a reference
pixel (the ecology reference height is 1080 pixels).

## Prompt set

Nest generation, with the approved pet icon as material reference and the old
cradle as concept reference:

> Production desktop pet game sprite, one empty minimalist dark capsule cradle
> nest on genuinely transparent background, 1536x1024 landscape. Redesign more
> minimal and friendly: smooth continuous charcoal violet satin ceramic crescent
> bowl, gentle broad violet/cyan pearlescent reflections, no ornate chrome, no
> orange/gold trim, no lamps, no stars, no character, no ball. Pale lavender/ivory
> soft single quiltless cushion with rounded side bolsters, large smooth forms.
> Orthographic front slightly elevated camera. Rear lip higher than a low curved
> front lip. Foreground rim and cushion form a clean shallow U-shaped boundary.
> Transparent actual alpha, no background, no typography.

Selected nest refinement:

> Keep the exact same nest shape, pixel position, size, camera, cushion geometry
> and framing. Remove exterior halo, haze and glow. Reduce reflection intensity
> on the outer shell by about 65%, darken shell to satin charcoal violet, soft
> diffuse highlights not bright neon streaks. Retain pale lavender offwhite
> cushion. Only nest on transparent background, no ground shadow.

Toy generation, approved pet icon as material reference:

> One production game sprite of a small pearl toy ball matching the material of
> the reference pet. Exact centered circular sphere, no face, no eyes, no mouth,
> no star, no symbols, no pattern, no stand. Very soft milky white pearlescent
> subsurface material with faint lavender, pale cyan and blush transitions,
> broad smooth light from upper left, restrained polished specular and gentle
> dimensional shading. Luminous but solid, not a hollow soap bubble. Transparent
> alpha outside physical circle, no external aura, no shadow, no background.

Final nest refinement after the user requested an even softer style:

> Make the entire cradle visibly softer: soft charcoal lavender silicone-foam
> outer capsule, gently pillowy yielding inflated edges, almost matte suede-like
> satin surface, broad diffuse lavender pearl shading. No metal, chrome, sharp
> bright rim, hard shiny streaks or texture pattern. Inner cushion more plush
> and puffy with gentle soft compression where it touches rounded side bolsters.
> One continuous cozy padded organic form. Preserve canvas, camera, overall
> silhouette and low front lip, keep center open for a seated pet. Genuine
> transparent alpha background, no exterior halo, no ground plane, no ball.

Generation output sizes were retained as returned, without resampling. A new
toy UV mapping fits its visible circle to the existing physical radius.
