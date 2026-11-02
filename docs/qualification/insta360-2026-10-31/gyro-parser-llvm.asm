Independent LLVM disassembly supplement; start/stop options were ignored by this Mach-O mode. Full output retained outside the repo.
__ZN10extra_info14GyroDataParser8ReadNextEPNS_10MotionItemE:
101b5d778:	08 30 40 f9	ldr	x8, [x0, #0x60]
101b5d77c:	1f e1 00 f1	cmp	x8, #0x38
101b5d780:	21 01 00 54	b.ne	0x101b5d7a4
101b5d784:	08 a8 40 a9	ldp	x8, x10, [x0, #0x8]
101b5d788:	09 34 80 b9	ldrsw	x9, [x0, #0x34]
101b5d78c:	0b 07 80 52	mov	w11, #0x38
101b5d790:	2b 7d 2b 9b	smull	x11, w9, w11
101b5d794:	6b e1 00 91	add	x11, x11, #0x38
101b5d798:	4a 01 08 cb	sub	x10, x10, x8
101b5d79c:	5f 01 0b eb	cmp	x10, x11
101b5d7a0:	62 00 00 54	b.hs	0x101b5d7ac
101b5d7a4:	00 00 80 52	mov	w0, #0x0
101b5d7a8:	c0 03 5f d6	ret
101b5d7ac:	0a 07 80 52	mov	w10, #0x38
101b5d7b0:	28 21 2a 9b	smaddl	x8, w9, w10, x8
101b5d7b4:	00 05 40 ad	ldp	q0, q1, [x8]
101b5d7b8:	02 09 c0 3d	ldr	q2, [x8, #0x20]
101b5d7bc:	08 19 40 f9	ldr	x8, [x8, #0x30]
101b5d7c0:	28 18 00 f9	str	x8, [x1, #0x30]
101b5d7c4:	21 88 00 ad	stp	q1, q2, [x1, #0x10]
101b5d7c8:	20 00 80 3d	str	q0, [x1]
101b5d7cc:	08 34 40 b9	ldr	w8, [x0, #0x34]
101b5d7d0:	08 05 00 11	add	w8, w8, #0x1
101b5d7d4:	08 34 00 b9	str	w8, [x0, #0x34]
101b5d7d8:	28 00 40 f9	ldr	x8, [x1]
101b5d7dc:	08 1c 00 f9	str	x8, [x0, #0x38]
101b5d7e0:	20 00 80 52	mov	w0, #0x1
101b5d7e4:	c0 03 5f d6	ret
__ZN10extra_info14GyroDataParser8ReadNextEPNS_11RawGyroItemE:
101b5d7e8:	08 30 40 f9	ldr	x8, [x0, #0x60]
101b5d7ec:	1f 51 00 f1	cmp	x8, #0x14
101b5d7f0:	21 01 00 54	b.ne	0x101b5d814
101b5d7f4:	08 a8 40 a9	ldp	x8, x10, [x0, #0x8]
101b5d7f8:	09 34 80 b9	ldrsw	x9, [x0, #0x34]
101b5d7fc:	8b 02 80 52	mov	w11, #0x14
101b5d800:	2b 7d 2b 9b	smull	x11, w9, w11
101b5d804:	6b 51 00 91	add	x11, x11, #0x14
101b5d808:	4a 01 08 cb	sub	x10, x10, x8
101b5d80c:	5f 01 0b eb	cmp	x10, x11
101b5d810:	62 00 00 54	b.hs	0x101b5d81c
101b5d814:	00 00 80 52	mov	w0, #0x0
101b5d818:	c0 03 5f d6	ret
101b5d81c:	8a 02 80 52	mov	w10, #0x14
101b5d820:	28 21 2a 9b	smaddl	x8, w9, w10, x8
101b5d824:	00 01 c0 3d	ldr	q0, [x8]
101b5d828:	08 11 40 b9	ldr	w8, [x8, #0x10]
101b5d82c:	28 10 00 b9	str	w8, [x1, #0x10]
101b5d830:	20 00 80 3d	str	q0, [x1]
101b5d834:	08 34 40 b9	ldr	w8, [x0, #0x34]
101b5d838:	08 05 00 11	add	w8, w8, #0x1
101b5d83c:	08 34 00 b9	str	w8, [x0, #0x34]
101b5d840:	28 00 40 f9	ldr	x8, [x1]
101b5d844:	08 1c 00 f9	str	x8, [x0, #0x38]
101b5d848:	20 00 80 52	mov	w0, #0x1
101b5d84c:	c0 03 5f d6	ret
