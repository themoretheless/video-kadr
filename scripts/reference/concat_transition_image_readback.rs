// SPDX-License-Identifier: AGPL-3.0-or-later
// Qualification-only extension appended to an isolated copy of pinned gpu.rs.
// Original bytes/pipeline/encode_transition and package WGSL remain unchanged.
impl WgpuCompositor {
    pub fn qualification_transition_image(&mut self, pass:&TransitionPass, width:u32, height:u32, from:&[[f32;4]], to:&[[f32;4]]) -> Option<Vec<[f32;4]>> {
        if self.dead || self.work!=WORK || width==0 || height==0 || width>64 || height>64 || from.len()!=(width*height)as usize || to.len()!=from.len(){return None;}
        self.used.values_mut().for_each(|used|*used=0);self.composites+=1;
        let fi=self.claim(width,height);let ti=self.claim(width,height);let oi=self.claim(width,height);
        for(index,pixels)in[(fi,from),(ti,to)]{
            let bytes:Vec<u8>=pixels.iter().flat_map(|p|p.iter().flat_map(|v|half::f16::from_f32(*v).to_le_bytes())).collect();
            self.queue.write_texture(wgpu::TexelCopyTextureInfo{texture:&self.pool[&(width,height)][index].texture,mip_level:0,origin:wgpu::Origin3d::ZERO,aspect:wgpu::TextureAspect::All},&bytes,wgpu::TexelCopyBufferLayout{offset:0,bytes_per_row:Some(width*8),rows_per_image:Some(height)},wgpu::Extent3d{width,height,depth_or_array_layers:1});
        }
        let view=|i:usize|self.pool[&(width,height)][i].texture.create_view(&wgpu::TextureViewDescriptor::default());
        let(fv,tv,ov)=(view(fi),view(ti),view(oi));
        let mut encoder=self.encode_transition(width,height,0.,&fv,&tv,pass,&ov)?;
        let stride=(width*8).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)*wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer=self.device.create_buffer(&wgpu::BufferDescriptor{label:Some("qualification image readback"),size:stride as u64*height as u64,usage:wgpu::BufferUsages::COPY_DST|wgpu::BufferUsages::MAP_READ,mapped_at_creation:false});
        encoder.copy_texture_to_buffer(wgpu::TexelCopyTextureInfo{texture:&self.pool[&(width,height)][oi].texture,mip_level:0,origin:wgpu::Origin3d::ZERO,aspect:wgpu::TextureAspect::All},wgpu::TexelCopyBufferInfo{buffer:&buffer,layout:wgpu::TexelCopyBufferLayout{offset:0,bytes_per_row:Some(stride),rows_per_image:Some(height)}},wgpu::Extent3d{width,height,depth_or_array_layers:1});
        self.queue.submit([encoder.finish()]);let slice=buffer.slice(..);let(tx,rx)=std::sync::mpsc::channel();slice.map_async(wgpu::MapMode::Read,move|result|{let _=tx.send(result);});
        let waited=self.wait();self.retire();if waited.is_err(){self.dead=true;return None;}if !matches!(rx.try_recv(),Ok(Ok(()))){return None;}
        let data=slice.get_mapped_range().ok()?;let mut out=Vec::with_capacity((width*height)as usize);
        for row in data.chunks_exact(stride as usize).take(height as usize){for pixel in row[..width as usize*8].chunks_exact(8){out.push(std::array::from_fn(|k|half::f16::from_le_bytes([pixel[k*2],pixel[k*2+1]]).to_f32()));}}
        drop(data);buffer.unmap();Some(out)
    }
}
