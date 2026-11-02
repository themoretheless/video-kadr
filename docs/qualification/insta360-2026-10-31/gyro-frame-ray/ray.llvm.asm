101b59c70:	ff c3 01 d1	sub	sp, sp, #0x70
101b59c74:	eb 2b 02 6d	stp	d11, d10, [sp, #0x20]
101b59c78:	e9 23 03 6d	stp	d9, d8, [sp, #0x30]
101b59c7c:	f6 57 04 a9	stp	x22, x21, [sp, #0x40]
101b59c80:	f4 4f 05 a9	stp	x20, x19, [sp, #0x50]
101b59c84:	fd 7b 06 a9	stp	x29, x30, [sp, #0x60]
101b59c88:	fd 83 01 91	add	x29, sp, #0x60
101b59c8c:	f3 03 03 aa	mov	x19, x3
101b59c90:	f5 03 02 aa	mov	x21, x2
101b59c94:	f4 03 00 aa	mov	x20, x0
101b59c98:	04 00 42 6d	ldp	d4, d0, [x0, #0x20]
101b59c9c:	08 a3 85 d2	mov	x8, #0x2d18
101b59ca0:	88 88 aa f2	movk	x8, #0x5444, lsl #16
101b59ca4:	68 3f c4 f2	movk	x8, #0x21fb, lsl #32
101b59ca8:	28 ff e7 f2	movk	x8, #0x3ff9, lsl #48
101b59cac:	01 01 67 9e	fmov	d1, x8
101b59cb0:	23 08 40 6d	ldp	d3, d2, [x1]
101b59cb4:	00 84 42 1f	fmsub	d0, d0, d2, d1
101b59cb8:	08 a3 85 d2	mov	x8, #0x2d18
101b59cbc:	88 88 aa f2	movk	x8, #0x5444, lsl #16
101b59cc0:	68 3f c4 f2	movk	x8, #0x21fb, lsl #32
101b59cc4:	28 03 e8 f2	movk	x8, #0x4019, lsl #48
101b59cc8:	01 01 67 9e	fmov	d1, x8
101b59ccc:	02 0c 40 fd	ldr	d2, [x0, #0x18]
101b59cd0:	21 38 62 1e	fsub	d1, d1, d2
101b59cd4:	68 84 44 1f	fmsub	d8, d3, d4, d1
101b59cd8:	b8 22 64 94	bl	0x1034627b8 ; symbol stub for: ___sincos_stret
101b59cdc:	09 40 60 1e	fmov	d9, d0
101b59ce0:	2a 40 60 1e	fmov	d10, d1
101b59ce4:	00 41 60 1e	fmov	d0, d8
101b59ce8:	b4 22 64 94	bl	0x1034627b8 ; symbol stub for: ___sincos_stret
101b59cec:	41 09 61 1e	fmul	d1, d10, d1
101b59cf0:	40 09 60 1e	fmul	d0, d10, d0
101b59cf4:	a3 0a 40 6d	ldp	d3, d2, [x21]
101b59cf8:	64 88 69 1e	fnmul	d4, d3, d9
101b59cfc:	a5 1a 41 6d	ldp	d5, d6, [x21, #0x10]
101b59d00:	a4 10 41 1f	fmadd	d4, d5, d1, d4
101b59d04:	47 88 61 1e	fnmul	d7, d2, d1
101b59d08:	01 04 18 6e	mov.d	v1[1], v0[0]
101b59d0c:	b0 40 61 1e	fneg	d16, d5
101b59d10:	b1 88 60 1e	fnmul	d17, d5, d0
101b59d14:	51 44 49 1f	fmadd	d17, d2, d9, d17
101b59d18:	60 1c 40 1f	fmadd	d0, d3, d0, d7
101b59d1c:	47 40 61 1e	fneg	d7, d2
101b59d20:	91 04 18 6e	mov.d	v17[1], v4[0]
101b59d24:	24 d6 71 4e	fadd.2d	v4, v17, v17
101b59d28:	00 28 60 1e	fadd	d0, d0, d0
101b59d2c:	10 9a c4 5f	fmul.d	d16, d16, v4[1]
101b59d30:	42 40 40 1f	fmadd	d2, d2, d0, d16
101b59d34:	70 88 60 1e	fnmul	d16, d3, d0
101b59d38:	b0 10 c4 5f	fmla.d	d16, d5, v4[0]
101b59d3c:	e5 08 64 1e	fmul	d5, d7, d4
101b59d40:	02 06 18 6e	mov.d	v2[1], v16[0]
101b59d44:	65 18 c4 5f	fmla.d	d5, d3, v4[1]
101b59d48:	83 90 c6 4f	fmul.2d	v3, v4, v6[0]
101b59d4c:	21 d4 63 4e	fadd.2d	v1, v1, v3
101b59d50:	21 d4 62 4e	fadd.2d	v1, v1, v2
101b59d54:	c0 08 60 1e	fmul	d0, d6, d0
101b59d58:	20 29 60 1e	fadd	d0, d9, d0
101b59d5c:	00 28 65 1e	fadd	d0, d0, d5
101b59d60:	e1 03 80 3d	str	q1, [sp]
101b59d64:	e0 0b 00 fd	str	d0, [sp, #0x10]
101b59d68:	80 06 40 f9	ldr	x0, [x20, #0x8]
101b59d6c:	08 00 40 f9	ldr	x8, [x0]
101b59d70:	08 15 40 f9	ldr	x8, [x8, #0x28]
101b59d74:	e1 03 00 91	mov	x1, sp
101b59d78:	e2 03 13 aa	mov	x2, x19
101b59d7c:	00 01 3f d6	blr	x8
101b59d80:	80 22 40 fd	ldr	d0, [x20, #0x40]
101b59d84:	61 0a 40 6d	ldp	d1, d2, [x19]
101b59d88:	00 08 61 1e	fmul	d0, d0, d1
101b59d8c:	60 02 00 fd	str	d0, [x19]
101b59d90:	80 26 40 fd	ldr	d0, [x20, #0x48]
101b59d94:	00 08 62 1e	fmul	d0, d0, d2
101b59d98:	60 06 00 fd	str	d0, [x19, #0x8]
101b59d9c:	fd 7b 46 a9	ldp	x29, x30, [sp, #0x60]
101b59da0:	f4 4f 45 a9	ldp	x20, x19, [sp, #0x50]
101b59da4:	f6 57 44 a9	ldp	x22, x21, [sp, #0x40]
101b59da8:	e9 23 43 6d	ldp	d9, d8, [sp, #0x30]
101b59dac:	eb 2b 42 6d	ldp	d11, d10, [sp, #0x20]
101b59db0:	ff c3 01 91	add	sp, sp, #0x70
101b59db4:	c0 03 5f d6	ret
