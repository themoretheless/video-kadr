#!/usr/bin/env python3
"""Run the pinned original Concat Metal compositor in an isolated qualification crate."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

PIN='78e4be101c2d3c41a7dc73c112c6b1f11b9ce170'

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--effect',choices=['glitch_warp','glitch_transition','cube','film_burn','zoom_blur_transition','light_leak_transition','light_leak_flash','zoom_transition','whip_pan','blur_dissolve','spin','swirl','rgb_split_wipe','push','slide','shape_wipe','iris','clock_wipe','linear_wipe','dissolve','fade_black','fade_white','bloom_pulse','glow','film_grain','halftone','motion_blur','zoom_blur','fisheye','chromatic_aberration','gaussian_blur','posterize','pixelate','luma_key','chroma_key','vhs','falling_dust','film','digicam','mono','midnight','marquee','retro_cartoon','super_8','contrast','exposure','saturation','vibrance','white_balance','color_wheels','curves','adjust','sharpen','vignette','contrast_pop','bleach_bypass','autumn','neon','pink_fuzz','night','spooky','cool','warm','teal_orange','noir','dreamy','high_contrast_mono','landscape_pop','natural','retro_70s','silver','tropical','vaporwave','winter_cool','classic_chrome','cyberpunk','delicious','rosy','sci_fi','clean','matcha','sage','ivory','milk','bronze','cool_blue','fresh','vivid','soft_light','moody','rain','umber','fair','peach','polaroid','vintage','cloudless_sky','lagoon','golden_hour','daydream'],default='contrast')
    parser.add_argument('--overshoot',action='store_true',help='Original Gaussian Blur runtime radius outside authored slider bounds')
    parser.add_argument('--fractional',action='store_true',help='Independent fractional-parameter validation image cases')
    parser.add_argument('--opaque',action='store_true',help='Opaque original Vivid spatial reference for ordinary lossy delivery')
    parser.add_argument('--zero-alpha',action='store_true',help='Original Sharpen chains with fully transparent colored neighbors')
    parser.add_argument('--spatial',action='store_true',help='Supported spatial package on a synthetic image, unchanged between input/readback passes')
    parser.add_argument('--media',action='store_true',help='Original Metal media pixel reference (Curves or Contrast Pop)')
    parser.add_argument('--fixed-uv',action='store_true',help='Fixed uniform UV texture sampling diagnostic; requires rotation diagnostic')
    parser.add_argument('--diagnostic',action='store_true',help='Instrument helper values; never an unmodified package parity oracle')
    parser.add_argument('--reference',required=True,type=Path)
    parser.add_argument('--output',required=True,type=Path,help='Unregistered oracle artifact; copy into tests only after Cargo exits')
    parser.add_argument('--target',required=True,type=Path)
    args=parser.parse_args()
    reference=args.reference.resolve()
    commit=subprocess.check_output(['git','-C',str(reference),'rev-parse','HEAD'],text=True).strip()
    if commit!=PIN:raise SystemExit('wrong reference commit')
    paths=['src/crates/concat-core','src/crates/concat-project','src/crates/concat-render','src/crates/concat-effects','src/Cargo.toml','src/Cargo.lock']
    def clean():
        if subprocess.check_output(['git','-C',str(reference),'status','--porcelain','--untracked-files=all','--',*paths],text=True):raise SystemExit('reference source is dirty')
    clean()
    repo=Path(__file__).resolve().parent.parent
    output=args.output.resolve()
    if repo.joinpath('backend/tests').resolve() in output.parents:raise SystemExit('generate into an unregistered artifact, not a live Rust test fixture')
    output.parent.mkdir(parents=True,exist_ok=True)
    if args.effect in ['glitch_warp','glitch_transition','cube','film_burn','zoom_blur_transition','light_leak_transition','light_leak_flash','zoom_transition','whip_pan','blur_dissolve','rgb_split_wipe','spin','swirl'] and not(args.spatial or (args.diagnostic and args.effect in ['glitch_transition','zoom_transition','spin','swirl'])):raise SystemExit('transition qualification requires spatial textures')
    if args.opaque and not(args.spatial and args.effect in ['vhs','falling_dust','film','digicam','mono','midnight','marquee','retro_cartoon','super_8','vivid','soft_light','rain','peach','polaroid','daydream','dreamy','night','neon','pink_fuzz']):raise SystemExit('opaque qualification requires spatial vivid')
    if args.zero_alpha and not(args.spatial and args.effect=='sharpen'):raise SystemExit('zero-alpha qualification requires spatial sharpen')
    if args.spatial and (args.effect not in ['glitch_warp','glitch_transition','cube','film_burn','zoom_blur_transition','light_leak_transition','light_leak_flash','zoom_transition','whip_pan','blur_dissolve','spin','swirl','rgb_split_wipe','bloom_pulse','glow','film_grain','halftone','motion_blur','zoom_blur','fisheye','chromatic_aberration','gaussian_blur','pixelate','vhs','falling_dust','film','digicam','mono','midnight','marquee','retro_cartoon','super_8','adjust','sharpen','vignette','vivid','soft_light','moody','rain','peach','polaroid','vintage','daydream','noir','dreamy','night','spooky','neon','pink_fuzz'] or args.media or args.diagnostic):raise SystemExit('spatial oracle supports only uninstrumented adjust, sharpen, or vignette')
    if args.media and (args.effect not in ['curves','contrast_pop','bleach_bypass','autumn','cool','warm','teal_orange','high_contrast_mono','landscape_pop','natural','retro_70s','silver','tropical','vaporwave','winter_cool','classic_chrome','cyberpunk','delicious','rosy','sci_fi','clean','matcha','sage','ivory','milk','bronze','cool_blue','fresh','umber','fair','cloudless_sky','lagoon','golden_hour'] or args.diagnostic):raise SystemExit('media oracle supports only uninstrumented curves or contrast_pop')
    if args.diagnostic and args.effect not in ['glitch_warp','glitch_transition','cube','film_burn','zoom_blur_transition','light_leak_transition','light_leak_flash','zoom_transition','spin','swirl','clock_wipe','linear_wipe','zoom_blur','white_balance','digicam','chroma_key','pixelate']:raise SystemExit('diagnostic adapter supports white_balance, digicam, chroma_key, or pixelate')
    adapter=Path(__file__).resolve().parent/'reference'/(f'concat_{args.effect}_spatial.rs' if args.spatial else f'concat_{args.effect}_media.rs' if args.media else f'concat_{args.effect}_diagnostic.rs' if args.diagnostic else f'concat_{args.effect}_gpu.rs')
    if args.effect in ['spin','swirl'] and (args.spatial or args.diagnostic):
        adapter=Path(__file__).resolve().parent/'reference'/('concat_rotation_diagnostic.rs' if args.diagnostic else 'concat_rotation_spatial.rs')
    if args.effect=='glitch_warp':
        if not args.spatial or args.diagnostic:raise SystemExit('Glitch Warp requires uninstrumented spatial qualification')
        adapter=Path(__file__).resolve().parent/'reference'/'concat_glitch_warp_spatial.rs'
    if args.effect=='glitch_transition':
        if not(args.spatial or args.diagnostic):raise SystemExit('Glitch requires spatial or diagnostic qualification')
        adapter=Path(__file__).resolve().parent/'reference'/('concat_glitch_transition_diagnostic.rs' if args.diagnostic else 'concat_glitch_transition_spatial.rs')
    if args.effect=='cube':
        if not args.spatial or args.diagnostic:raise SystemExit('Cube requires uninstrumented spatial qualification')
        adapter=Path(__file__).resolve().parent/'reference'/'concat_cube_spatial.rs'
    if args.effect=='film_burn':
        if not args.spatial or args.diagnostic:raise SystemExit('Film Burn requires uninstrumented spatial qualification')
        adapter=Path(__file__).resolve().parent/'reference'/'concat_film_burn_spatial.rs'
    if args.effect=='zoom_blur_transition':
        if not args.spatial or args.diagnostic:raise SystemExit('Zoom Blur transition requires uninstrumented spatial qualification')
        adapter=Path(__file__).resolve().parent/'reference'/'concat_zoom_blur_transition_spatial.rs'
    if args.effect=='light_leak_transition':
        if not args.spatial or args.diagnostic:raise SystemExit('Light Leak requires uninstrumented spatial qualification')
        adapter=Path(__file__).resolve().parent/'reference'/'concat_light_leak_transition_spatial.rs'
    if args.effect=='light_leak_flash':
        if not args.spatial or args.diagnostic:raise SystemExit('Light Leak Flash requires uninstrumented spatial qualification')
        adapter=Path(__file__).resolve().parent/'reference'/'concat_light_leak_flash_spatial.rs'
    if args.effect=='zoom_transition':
        if not(args.spatial or args.diagnostic):raise SystemExit('Zoom requires spatial or diagnostic qualification')
        adapter=Path(__file__).resolve().parent/'reference'/('concat_zoom_transition_diagnostic.rs' if args.diagnostic else 'concat_zoom_transition_spatial.rs')
    if args.effect=='whip_pan':
        if not args.spatial or args.diagnostic:raise SystemExit('Whip Pan requires uninstrumented spatial qualification')
        adapter=Path(__file__).resolve().parent/'reference'/'concat_whip_pan_spatial.rs'
    if args.effect=='blur_dissolve':
        if not args.spatial or args.diagnostic:raise SystemExit('Blur Dissolve requires uninstrumented spatial qualification')
        adapter=Path(__file__).resolve().parent/'reference'/'concat_blur_dissolve_spatial.rs'
    if args.fixed_uv:
        if not(args.diagnostic and args.effect in ['spin','swirl']):raise SystemExit('fixed UV requires rotation diagnostic')
        adapter=Path(__file__).resolve().parent/'reference'/'concat_rotation_fixed_uv_diagnostic.rs'
    if args.effect in ['push','slide']:
        if args.spatial or args.media or args.diagnostic:raise SystemExit('movement uses direct original centre probe')
        adapter=Path(__file__).resolve().parent/'reference'/'concat_movement_gpu.rs'
    if args.effect == 'clock_wipe':
        if args.spatial or args.media:raise SystemExit('clock wipe uses direct centre probe')
        adapter=Path(__file__).resolve().parent/'reference'/('concat_clock_wipe_diagnostic.rs' if args.diagnostic else 'concat_clock_wipe_gpu.rs')
    if args.effect == 'linear_wipe':
        if args.spatial or args.media:raise SystemExit('wipe uses direct two-input probe')
        adapter=Path(__file__).resolve().parent/'reference'/('concat_linear_wipe_diagnostic.rs' if args.diagnostic else 'concat_linear_wipe_gpu.rs')
    if args.effect in ['dissolve','fade_black','fade_white']:
        if args.spatial or args.media or args.diagnostic:raise SystemExit('transition uses direct unmodified two-input probe')
        adapter=Path(__file__).resolve().parent/'reference'/'concat_transition_gpu.rs'
    if args.effect in ['sharpen','vignette']:adapter=Path(__file__).resolve().parent/'reference'/('concat_spatial_filter_image.rs' if args.spatial else 'concat_spatial_filter_gpu.rs')
    if args.overshoot:
        if not(args.spatial and args.effect=='gaussian_blur'):raise SystemExit('overshoot requires Gaussian spatial oracle')
        adapter=Path(__file__).resolve().parent/'reference'/'concat_gaussian_blur_overshoot_spatial.rs'
    if args.fractional:
        if args.effect=='glitch_warp' and args.spatial and not(args.media or args.diagnostic or args.opaque):
            adapter=Path(__file__).resolve().parent/'reference'/'concat_glitch_warp_fractional_spatial.rs'
        elif args.effect=='glitch_transition' and args.spatial and not(args.media or args.diagnostic or args.opaque):
            adapter=Path(__file__).resolve().parent/'reference'/'concat_glitch_transition_fractional_spatial.rs'
        elif args.effect=='cube' and args.spatial and not(args.media or args.diagnostic or args.opaque):
            adapter=Path(__file__).resolve().parent/'reference'/'concat_cube_fractional_spatial.rs'
        elif args.effect=='film_burn' and args.spatial and not(args.media or args.diagnostic or args.opaque):
            adapter=Path(__file__).resolve().parent/'reference'/'concat_film_burn_fractional_spatial.rs'
        elif args.effect=='zoom_blur_transition' and args.spatial and not(args.media or args.diagnostic or args.opaque):
            adapter=Path(__file__).resolve().parent/'reference'/'concat_zoom_blur_transition_fractional_spatial.rs'
        elif args.effect=='light_leak_transition' and args.spatial and not(args.media or args.diagnostic or args.opaque):
            adapter=Path(__file__).resolve().parent/'reference'/'concat_light_leak_transition_fractional_spatial.rs'
        elif args.effect=='light_leak_flash' and args.spatial and not(args.media or args.diagnostic or args.opaque):
            adapter=Path(__file__).resolve().parent/'reference'/'concat_light_leak_flash_fractional_spatial.rs'
        elif args.effect=='zoom_transition' and args.spatial and not(args.media or args.diagnostic or args.opaque):
            adapter=Path(__file__).resolve().parent/'reference'/'concat_zoom_transition_fractional_spatial.rs'
        elif args.effect=='whip_pan' and args.spatial and not(args.media or args.diagnostic or args.opaque):
            adapter=Path(__file__).resolve().parent/'reference'/'concat_whip_pan_fractional_spatial.rs'
        elif args.effect=='blur_dissolve' and args.spatial and not(args.media or args.diagnostic or args.opaque):
            adapter=Path(__file__).resolve().parent/'reference'/'concat_blur_dissolve_fractional_spatial.rs'
        elif args.effect in ['spin','swirl'] and args.spatial and not(args.media or args.diagnostic or args.opaque):
            adapter=Path(__file__).resolve().parent/'reference'/'concat_rotation_fractional_spatial.rs'
        elif args.effect=='rgb_split_wipe' and args.spatial and not(args.media or args.diagnostic or args.opaque):
            adapter=Path(__file__).resolve().parent/'reference'/'concat_rgb_split_wipe_fractional_spatial.rs'
        elif args.effect in ['push','slide'] and not(args.spatial or args.media or args.diagnostic or args.opaque):
            adapter=Path(__file__).resolve().parent/'reference'/'concat_movement_fractional_gpu.rs'
        elif args.effect in ['clock_wipe','iris','shape_wipe'] and not(args.spatial or args.media or args.diagnostic or args.opaque):
            adapter=Path(__file__).resolve().parent/'reference'/f'concat_{args.effect}_fractional_gpu.rs'
        elif args.effect == 'linear_wipe' and not(args.spatial or args.media or args.diagnostic or args.opaque):
            adapter=Path(__file__).resolve().parent/'reference'/'concat_linear_wipe_fractional_gpu.rs'
        else:
            if not(args.effect in ['vhs','film_grain'] and args.spatial and not args.opaque):raise SystemExit('fractional validation requires supported spatial effect')
            adapter=Path(__file__).resolve().parent/'reference'/f'concat_{args.effect}_fractional_spatial.rs'
    if args.opaque:adapter=Path(__file__).resolve().parent/'reference'/f'concat_{args.effect}_opaque_spatial.rs'
    if args.zero_alpha:adapter=Path(__file__).resolve().parent/'reference'/'concat_zero_alpha_image.rs'
    tracked=['src/Cargo.lock','src/crates/concat-render/src/gpu.rs','src/crates/concat-effects/src/shader.rs',f'src/crates/concat-effects/packages/concat.{args.effect.replace('_','-')}/effect.toml',f'src/crates/concat-effects/packages/concat.{args.effect.replace('_','-')}/effect.wgsl']
    if (args.spatial and args.effect!='adjust') or (args.diagnostic and args.effect in ['digicam','zoom_blur']):tracked+=['src/crates/concat-effects/packages/concat.adjust/effect.toml','src/crates/concat-effects/packages/concat.adjust/effect.wgsl']
    if args.zero_alpha:
        for package in ['exposure','vignette','curves']:tracked += [f'src/crates/concat-effects/packages/concat.{package}/effect.toml',f'src/crates/concat-effects/packages/concat.{package}/effect.wgsl']
    fingerprints={name:hashlib.sha256((reference/name).read_bytes()).hexdigest() for name in tracked}
    with tempfile.TemporaryDirectory(prefix='video-kadr-concat-gpu-') as scratch:
        root=Path(scratch);(root/'src').mkdir();shutil.copyfile(adapter,root/'src/main.rs');shutil.copyfile(reference/'src/Cargo.lock',root/'Cargo.lock')
        render_path=reference/'src/crates/concat-render'
        extension=None
        if args.effect in ['glitch_warp','glitch_transition','cube','film_burn','zoom_blur_transition','light_leak_transition','light_leak_flash','zoom_transition','whip_pan','blur_dissolve','rgb_split_wipe','spin','swirl'] and (args.spatial or args.diagnostic):
            render_path=root/'concat-render'
            shutil.copytree(reference/'src/crates/concat-render/src',render_path/'src')
            (render_path/'Cargo.toml').write_text('[package]\nname="concat-render"\nversion="0.2.7"\nedition="2024"\nrust-version="1.93"\nlicense="AGPL-3.0-or-later"\n[features]\ngpu=[]\n[dependencies]\nconcat-core={path='+json.dumps(str(reference/'src/crates/concat-core'))+'}\nlog="0.4"\nwgpu="30"\npollster="0.4"\nhalf="2"\n')
            extension=(Path(__file__).resolve().parent/'reference'/'concat_transition_image_readback.rs').read_bytes()
            original_gpu=(reference/'src/crates/concat-render/src/gpu.rs').read_bytes()
            (render_path/'src/gpu.rs').write_bytes(original_gpu+b'\n'+extension)
            assert (render_path/'src/gpu.rs').read_bytes()==original_gpu+b'\n'+extension
        dependencies='\n'.join(name+' = { path = '+json.dumps(str(reference/'src/crates'/name) if name!='concat-render' else str(render_path))+' }' for name in ['concat-effects','concat-render'])
        (root/'Cargo.toml').write_text('[package]\nname="video-kadr-concat-gpu-oracle"\nversion="0.0.0"\nedition="2021"\n[workspace]\n[dependencies]\n'+dependencies+'\nserde_json="1"\nhalf="2"\n')
        env=os.environ.copy();env.update(CARGO_TARGET_DIR=str(args.target.resolve()),CARGO_INCREMENTAL='0',CARGO_PROFILE_DEV_DEBUG='0')
        subprocess.run(['cargo','+1.98.1','run','--manifest-path',str(root/'Cargo.toml'),'--',str(output),args.effect],env=env,check=True)
        clean()
        if fingerprints!={name:hashlib.sha256((reference/name).read_bytes()).hexdigest() for name in tracked}:raise SystemExit('reference changed during qualification')
        result=json.loads(output.read_text())
        if bool(result.get('instrumented',False))!=args.diagnostic:raise SystemExit('oracle/diagnostic result kind mismatch')
        if bool(result.get('opaqueInput',False))!=args.opaque:raise SystemExit('oracle alpha mode mismatch')
        if bool(result.get('overshootInput',False))!=args.overshoot:raise SystemExit('oracle radius mode mismatch')
        if extension is not None:
            result['readbackExtensionSha256']=hashlib.sha256(extension).hexdigest()
            result['rendererBodyUnchanged']=(render_path/'src/gpu.rs').read_bytes()==original_gpu+b'\n'+extension
        result['reference']={'repository':'https://github.com/jub0t/concat','commit':PIN,'files':fingerprints,'adapterSha256':hashlib.sha256(adapter.read_bytes()).hexdigest()}
        output.write_text(json.dumps(result,indent=2,allow_nan=False)+'\n')
    label="original Metal media oracle" if args.media else "instrumented Metal diagnostic" if args.diagnostic else "original Metal oracle"
    print(f'{label}: {len(result["cases"])} cases, {result["upstreamProbes"]} original probes; {output}')

if __name__=='__main__':main()
