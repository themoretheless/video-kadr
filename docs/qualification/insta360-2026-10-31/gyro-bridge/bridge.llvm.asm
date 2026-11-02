101036b60:	00 08 40 2d	ldp	s0, s2, [x0]
101036b64:	05 08 40 bd	ldr	s5, [x0, #0x8]
101036b68:	07 04 42 2d	ldp	s7, s1, [x0, #0x10]
101036b6c:	04 18 40 bd	ldr	s4, [x0, #0x18]
101036b70:	10 18 44 2d	ldp	s16, s6, [x0, #0x20]
101036b74:	03 28 40 bd	ldr	s3, [x0, #0x28]
101036b78:	11 38 21 1e	fsub	s17, s0, s1
101036b7c:	31 3a 23 1e	fsub	s17, s17, s3
101036b80:	32 38 20 1e	fsub	s18, s1, s0
101036b84:	52 3a 23 1e	fsub	s18, s18, s3
101036b88:	73 38 20 1e	fsub	s19, s3, s0
101036b8c:	73 3a 21 1e	fsub	s19, s19, s1
101036b90:	00 28 21 1e	fadd	s0, s0, s1
101036b94:	00 28 23 1e	fadd	s0, s0, s3
101036b98:	20 22 20 1e	fcmp	s17, s0
101036b9c:	e8 d7 9f 1a	cset	w8, gt
101036ba0:	20 ce 20 1e	fcsel	s0, s17, s0, gt
101036ba4:	40 22 20 1e	fcmp	s18, s0
101036ba8:	40 ce 20 1e	fcsel	s0, s18, s0, gt
101036bac:	49 00 80 52	mov	w9, #0x2
101036bb0:	28 c1 88 1a	csel	w8, w9, w8, gt
101036bb4:	60 22 20 1e	fcmp	s19, s0
101036bb8:	60 ce 20 1e	fcsel	s0, s19, s0, gt
101036bbc:	69 00 80 52	mov	w9, #0x3
101036bc0:	28 c1 88 1a	csel	w8, w9, w8, gt
101036bc4:	01 10 2e 1e	fmov	s1, #1.00000000
101036bc8:	00 28 21 1e	fadd	s0, s0, s1
101036bcc:	00 c0 21 1e	fsqrt	s0, s0
101036bd0:	01 10 2c 1e	fmov	s1, #0.50000000
101036bd4:	00 08 21 1e	fmul	s0, s0, s1
101036bd8:	01 10 2a 1e	fmov	s1, #0.25000000
101036bdc:	31 18 20 1e	fdiv	s17, s1, s0
101036be0:	e9 46 01 90	adrp	x9, 10460 ; 0x103912000
101036be4:	29 e5 09 91	add	x9, x9, #0x279
101036be8:	8a 00 00 10	adr	x10, #16
101036bec:	2b 69 68 38	ldrb	w11, [x9, x8]
101036bf0:	4a 09 0b 8b	add	x10, x10, x11, lsl #2
101036bf4:	40 01 1f d6	br	x10
101036bf8:	81 38 26 1e	fsub	s1, s4, s6
101036bfc:	03 40 20 1e	fmov	s3, s0
101036c00:	20 08 31 1e	fmul	s0, s1, s17
101036c04:	01 3a 25 1e	fsub	s1, s16, s5
101036c08:	21 08 31 1e	fmul	s1, s1, s17
101036c0c:	42 38 27 1e	fsub	s2, s2, s7
101036c10:	42 08 31 1e	fmul	s2, s2, s17
101036c14:	c0 03 5f d6	ret
101036c18:	01 3a 25 1e	fsub	s1, s16, s5
101036c1c:	23 08 31 1e	fmul	s3, s1, s17
101036c20:	42 28 27 1e	fadd	s2, s2, s7
101036c24:	01 40 20 1e	fmov	s1, s0
101036c28:	40 08 31 1e	fmul	s0, s2, s17
101036c2c:	82 28 26 1e	fadd	s2, s4, s6
101036c30:	42 08 31 1e	fmul	s2, s2, s17
101036c34:	c0 03 5f d6	ret
101036c38:	41 38 27 1e	fsub	s1, s2, s7
101036c3c:	23 08 31 1e	fmul	s3, s1, s17
101036c40:	a1 28 30 1e	fadd	s1, s5, s16
101036c44:	02 40 20 1e	fmov	s2, s0
101036c48:	20 08 31 1e	fmul	s0, s1, s17
101036c4c:	81 28 26 1e	fadd	s1, s4, s6
101036c50:	21 08 31 1e	fmul	s1, s1, s17
101036c54:	c0 03 5f d6	ret
101036c58:	81 38 26 1e	fsub	s1, s4, s6
101036c5c:	23 08 31 1e	fmul	s3, s1, s17
101036c60:	41 28 27 1e	fadd	s1, s2, s7
101036c64:	21 08 31 1e	fmul	s1, s1, s17
101036c68:	a2 28 30 1e	fadd	s2, s5, s16
101036c6c:	42 08 31 1e	fmul	s2, s2, s17
101036c70:	c0 03 5f d6	ret
1018b928c:	48 8d 01 d0	adrp	x8, 12714 ; 0x104a63000
1018b9290:	08 c1 36 91	add	x8, x8, #0xdb0
1018b9294:	00 05 42 2d	ldp	s0, s1, [x8, #0x10]
1018b9298:	02 0d 43 2d	ldp	s2, s3, [x8, #0x18]
1018b929c:	04 14 40 2d	ldp	s4, s5, [x0]
1018b92a0:	06 88 24 1e	fnmul	s6, s0, s4
1018b92a4:	10 1c 41 2d	ldp	s16, s7, [x0, #0x8]
1018b92a8:	66 18 07 1f	fmadd	s6, s3, s7, s6
1018b92ac:	26 98 05 1f	fmsub	s6, s1, s5, s6
1018b92b0:	46 98 10 1f	fmsub	s6, s2, s16, s6
1018b92b4:	11 08 27 1e	fmul	s17, s0, s7
1018b92b8:	71 44 04 1f	fmadd	s17, s3, s4, s17
1018b92bc:	31 44 10 1f	fmadd	s17, s1, s16, s17
1018b92c0:	51 c4 05 1f	fmsub	s17, s2, s5, s17
1018b92c4:	32 08 27 1e	fmul	s18, s1, s7
1018b92c8:	72 48 05 1f	fmadd	s18, s3, s5, s18
1018b92cc:	52 48 04 1f	fmadd	s18, s2, s4, s18
1018b92d0:	12 c8 10 1f	fmsub	s18, s0, s16, s18
1018b92d4:	42 08 27 1e	fmul	s2, s2, s7
1018b92d8:	62 08 10 1f	fmadd	s2, s3, s16, s2
1018b92dc:	00 08 05 1f	fmadd	s0, s0, s5, s2
1018b92e0:	22 80 04 1f	fmsub	s2, s1, s4, s0
1018b92e4:	04 15 40 2d	ldp	s4, s5, [x8]
1018b92e8:	20 8a 24 1e	fnmul	s0, s17, s4
1018b92ec:	10 1d 41 2d	ldp	s16, s7, [x8, #0x8]
1018b92f0:	c0 00 07 1f	fmadd	s0, s6, s7, s0
1018b92f4:	40 82 05 1f	fmsub	s0, s18, s5, s0
1018b92f8:	43 80 10 1f	fmsub	s3, s2, s16, s0
1018b92fc:	e0 08 31 1e	fmul	s0, s7, s17
1018b9300:	c0 00 04 1f	fmadd	s0, s6, s4, s0
1018b9304:	40 02 10 1f	fmadd	s0, s18, s16, s0
1018b9308:	40 80 05 1f	fmsub	s0, s2, s5, s0
1018b930c:	e1 08 32 1e	fmul	s1, s7, s18
1018b9310:	c1 04 05 1f	fmadd	s1, s6, s5, s1
1018b9314:	41 04 04 1f	fmadd	s1, s2, s4, s1
1018b9318:	21 86 10 1f	fmsub	s1, s17, s16, s1
1018b931c:	e2 08 22 1e	fmul	s2, s7, s2
1018b9320:	c2 08 10 1f	fmadd	s2, s6, s16, s2
1018b9324:	22 0a 05 1f	fmadd	s2, s17, s5, s2
1018b9328:	42 8a 04 1f	fmsub	s2, s18, s4, s2
1018b932c:	c0 03 5f d6	ret
1018b9728:	ff 83 01 d1	sub	sp, sp, #0x60
1018b972c:	f4 4f 04 a9	stp	x20, x19, [sp, #0x40]
1018b9730:	fd 7b 05 a9	stp	x29, x30, [sp, #0x50]
1018b9734:	fd 43 01 91	add	x29, sp, #0x50
1018b9738:	e0 3a ff d0	adrp	x0, -6306 ; 0x100017000
1018b973c:	00 b0 2d 91	add	x0, x0, #0xb6c
1018b9740:	53 8d 01 d0	adrp	x19, 12714 ; 0x104a63000
1018b9744:	73 82 36 91	add	x19, x19, #0xda0
1018b9748:	22 3a ff f0	adrp	x2, -6329 ; 0x100000000
1018b974c:	42 00 00 91	add	x2, x2, #0x0
1018b9750:	e1 03 13 aa	mov	x1, x19
1018b9754:	d7 a3 6e 94	bl	0x1034626b0 ; symbol stub for: ___cxa_atexit
1018b9758:	28 13 01 90	adrp	x8, 8804 ; 0x103b1d000
1018b975c:	00 61 c2 3d	ldr	q0, [x8, #0x980]
1018b9760:	28 13 01 90	adrp	x8, 8804 ; 0x103b1d000
1018b9764:	01 65 c2 3d	ldr	q1, [x8, #0x990]
1018b9768:	e0 07 00 ad	stp	q0, q1, [sp]
1018b976c:	28 13 01 90	adrp	x8, 8804 ; 0x103b1d000
1018b9770:	00 d1 44 fd	ldr	d0, [x8, #0x9a0]
1018b9774:	e0 13 00 fd	str	d0, [sp, #0x20]
1018b9778:	c8 a5 97 52	mov	w8, #0xbd2e
1018b977c:	68 67 b6 72	movk	w8, #0xb33b, lsl #16
1018b9780:	e8 2b 00 b9	str	w8, [sp, #0x28]
1018b9784:	ff 43 03 f8	stur	xzr, [sp, #0x34]
1018b9788:	ff c3 02 f8	stur	xzr, [sp, #0x2c]
1018b978c:	08 f0 a7 52	mov	w8, #0x3f800000
1018b9790:	e8 3f 00 b9	str	w8, [sp, #0x3c]
1018b9794:	e0 03 00 91	mov	x0, sp
1018b9798:	f2 f4 dd 97	bl	0x101036b60
1018b979c:	60 06 02 2d	stp	s0, s1, [x19, #0x10]
1018b97a0:	62 0e 03 2d	stp	s2, s3, [x19, #0x18]
1018b97a4:	04 08 20 1e	fmul	s4, s0, s0
1018b97a8:	20 04 0c 6e	mov.s	v0[1], v1[0]
1018b97ac:	45 40 21 1e	fneg	s5, s2
1018b97b0:	21 08 21 1e	fmul	s1, s1, s1
1018b97b4:	42 08 22 1e	fmul	s2, s2, s2
1018b97b8:	66 08 23 1e	fmul	s6, s3, s3
1018b97bc:	81 28 21 1e	fadd	s1, s4, s1
1018b97c0:	42 28 26 1e	fadd	s2, s2, s6
1018b97c4:	21 28 22 1e	fadd	s1, s1, s2
1018b97c8:	00 f8 a0 6e	fneg.4s	v0, v0
1018b97cc:	a0 04 14 6e	mov.s	v0[2], v5[0]
1018b97d0:	60 04 1c 6e	mov.s	v0[3], v3[0]
1018b97d4:	21 04 04 4e	dup.4s	v1, v1[0]
1018b97d8:	00 fc 21 6e	fdiv.4s	v0, v0, v1
1018b97dc:	60 0a 80 3d	str	q0, [x19, #0x20]
1018b97e0:	fd 7b 45 a9	ldp	x29, x30, [sp, #0x50]
1018b97e4:	f4 4f 44 a9	ldp	x20, x19, [sp, #0x40]
1018b97e8:	ff 83 01 91	add	sp, sp, #0x60
1018b97ec:	c0 03 5f d6	ret
