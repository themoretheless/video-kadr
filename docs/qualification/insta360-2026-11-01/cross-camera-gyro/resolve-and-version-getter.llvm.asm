1018b4b30:	68 12 00 b9	str	w8, [x19, #0x10]
1018b4b34:	30 00 00 14	b	0x1018b4bf4
1018b4b38:	48 06 e8 d2	mov	x8, #0x4032000000000000
1018b4b3c:	68 02 00 f9	str	x8, [x19]
1018b5100:	f3 03 00 aa	mov	x19, x0
1018b5104:	e0 c3 00 91	add	x0, sp, #0x30
1018b5108:	b2 db 9d 97	bl	__ZN3ins11LLogMessageD1Ev
1018b510c:	e8 5f c0 39	ldrsb	w8, [sp, #0x17]
1018b5110:	e8 04 f8 37	tbnz	w8, #0x1f, 0x1018b51ac
1018b5114:	28 00 00 14	b	0x1018b51b4
1018b5118:	f3 03 00 aa	mov	x19, x0
1018b511c:	e8 5f c0 39	ldrsb	w8, [sp, #0x17]
1018b5120:	68 04 f8 37	tbnz	w8, #0x1f, 0x1018b51ac
1018b5124:	24 00 00 14	b	0x1018b51b4
1018b5128:	f3 03 00 aa	mov	x19, x0
1018b512c:	22 00 00 14	b	0x1018b51b4
1018b5130:	f3 03 00 aa	mov	x19, x0
1018b5134:	e0 c3 00 91	add	x0, sp, #0x30
1018b5138:	a6 db 9d 97	bl	__ZN3ins11LLogMessageD1Ev
1018b513c:	e8 5f c0 39	ldrsb	w8, [sp, #0x17]
1018b5140:	68 03 f8 37	tbnz	w8, #0x1f, 0x1018b51ac
1018b5144:	1c 00 00 14	b	0x1018b51b4
1018b5148:	f3 03 00 aa	mov	x19, x0
1018b514c:	e0 c3 00 91	add	x0, sp, #0x30
1018b5150:	a0 db 9d 97	bl	__ZN3ins11LLogMessageD1Ev
1018b5154:	e8 5f c0 39	ldrsb	w8, [sp, #0x17]
1018b5158:	a8 02 f8 37	tbnz	w8, #0x1f, 0x1018b51ac
1018b515c:	16 00 00 14	b	0x1018b51b4
1018b5160:	f3 03 00 aa	mov	x19, x0
1018b5164:	e8 5f c0 39	ldrsb	w8, [sp, #0x17]
1018b5168:	28 02 f8 37	tbnz	w8, #0x1f, 0x1018b51ac
1018b516c:	12 00 00 14	b	0x1018b51b4
1018b5170:	f3 03 00 aa	mov	x19, x0
1018b5174:	e0 c3 00 91	add	x0, sp, #0x30
1018b5178:	96 db 9d 97	bl	__ZN3ins11LLogMessageD1Ev
1018b517c:	e8 5f c0 39	ldrsb	w8, [sp, #0x17]
1018b5180:	68 01 f8 37	tbnz	w8, #0x1f, 0x1018b51ac
1018b5184:	0c 00 00 14	b	0x1018b51b4
1018b5188:	f3 03 00 aa	mov	x19, x0
1018b518c:	e0 c3 00 91	add	x0, sp, #0x30
1018b5190:	90 db 9d 97	bl	__ZN3ins11LLogMessageD1Ev
1018b5194:	e8 5f c0 39	ldrsb	w8, [sp, #0x17]
1018b5198:	a8 00 f8 37	tbnz	w8, #0x1f, 0x1018b51ac
1018b519c:	06 00 00 14	b	0x1018b51b4
1018b51a0:	f3 03 00 aa	mov	x19, x0
1018b51a4:	e8 5f c0 39	ldrsb	w8, [sp, #0x17]
1018b51a8:	68 00 f8 36	tbz	w8, #0x1f, 0x1018b51b4
1018b51ac:	e0 03 40 f9	ldr	x0, [sp]
1018b51b0:	da b4 6e 94	bl	0x103462518 ; symbol stub for: __ZdlPv
1018b51b4:	e8 bf c0 39	ldrsb	w8, [sp, #0x2f]
1018b51b8:	a8 00 f8 36	tbz	w8, #0x1f, 0x1018b51cc
1018b51bc:	e0 0f 40 f9	ldr	x0, [sp, #0x18]
1018b51c0:	d6 b4 6e 94	bl	0x103462518 ; symbol stub for: __ZdlPv
1018b51c4:	e0 03 13 aa	mov	x0, x19
1018b51c8:	ad 99 6e 94	bl	0x10345b87c ; symbol stub for: __Unwind_Resume
1018b51cc:	e0 03 13 aa	mov	x0, x19
1018b51d0:	ab 99 6e 94	bl	0x10345b87c ; symbol stub for: __Unwind_Resume
1018b51d4:	ff 83 07 d1	sub	sp, sp, #0x1e0
1018b51d8:	f6 57 1b a9	stp	x22, x21, [sp, #0x1b0]
1018b51dc:	f4 4f 1c a9	stp	x20, x19, [sp, #0x1c0]
1018b51e0:	fd 7b 1d a9	stp	x29, x30, [sp, #0x1d0]
1018b51e4:	fd 43 07 91	add	x29, sp, #0x1d0
1018b51e8:	08 5c 40 39	ldrb	w8, [x0, #0x17]
1018b51ec:	09 1d 00 13	sxtb	w9, w8
1018b51f0:	0a 04 40 f9	ldr	x10, [x0, #0x8]
1018b51f4:	3f 01 00 71	cmp	w9, #0x0
1018b51f8:	48 b1 88 9a	csel	x8, x10, x8, lt
1018b51fc:	48 02 00 b4	cbz	x8, 0x1018b5244
1018b5200:	f3 03 00 aa	mov	x19, x0
1018b5204:	e0 23 00 91	add	x0, sp, #0x8
1018b5208:	e3 6c 40 94	bl	__ZN3ins6OffsetC1Ev
1018b520c:	e0 23 00 91	add	x0, sp, #0x8
1018b5210:	e1 03 13 aa	mov	x1, x19
1018b5214:	8a 6d 40 94	bl	__ZN3ins6Offset13isValidOffsetERKNSt3__112basic_stringIcNS1_11char_traitsIcEENS1_9allocatorIcEEEE
1018b5218:	f5 03 00 aa	mov	x21, x0
1018b521c:	e0 23 00 91	add	x0, sp, #0x8
1018b5220:	8c a3 a5 97	bl	0x10021e050
1018b5224:	14 00 80 12	mov	w20, #-0x1
1018b5228:	d5 01 00 34	cbz	w21, 0x1018b5260
1018b522c:	e0 03 14 aa	mov	x0, x20
1018b5230:	fd 7b 5d a9	ldp	x29, x30, [sp, #0x1d0]
1018b5234:	f4 4f 5c a9	ldp	x20, x19, [sp, #0x1c0]
1018b5238:	f6 57 5b a9	ldp	x22, x21, [sp, #0x1b0]
1018b523c:	ff 83 07 91	add	sp, sp, #0x1e0
1018b5240:	c0 03 5f d6	ret
1018b5244:	14 00 80 12	mov	w20, #-0x1
1018b5248:	e0 03 14 aa	mov	x0, x20
1018b524c:	fd 7b 5d a9	ldp	x29, x30, [sp, #0x1d0]
1018b5250:	f4 4f 5c a9	ldp	x20, x19, [sp, #0x1c0]
1018b5254:	f6 57 5b a9	ldp	x22, x21, [sp, #0x1b0]
1018b5258:	ff 83 07 91	add	sp, sp, #0x1e0
1018b525c:	c0 03 5f d6	ret
1018b5260:	f4 07 00 b9	str	w20, [sp, #0x4]
1018b5264:	e0 23 00 91	add	x0, sp, #0x8
1018b5268:	cb 6c 40 94	bl	__ZN3ins6OffsetC1Ev
1018b526c:	e0 23 00 91	add	x0, sp, #0x8
1018b5270:	e1 03 13 aa	mov	x1, x19
1018b5274:	d4 6c 40 94	bl	__ZN3ins6Offset9setOffsetERKNSt3__112basic_stringIcNS1_11char_traitsIcEENS1_9allocatorIcEEEE
1018b5278:	e0 23 00 91	add	x0, sp, #0x8
1018b527c:	e1 13 00 91	add	x1, sp, #0x4
1018b5280:	43 89 40 94	bl	__ZN3ins6Offset10getVersionERNS_4Lens7LENSVERE
1018b5284:	f4 07 40 b9	ldr	w20, [sp, #0x4]
1018b5288:	e0 23 00 91	add	x0, sp, #0x8
1018b528c:	71 a3 a5 97	bl	0x10021e050
1018b5290:	e0 03 14 aa	mov	x0, x20
1018b5294:	fd 7b 5d a9	ldp	x29, x30, [sp, #0x1d0]
1018b5298:	f4 4f 5c a9	ldp	x20, x19, [sp, #0x1c0]
1018b529c:	f6 57 5b a9	ldp	x22, x21, [sp, #0x1b0]
1018b52a0:	ff 83 07 91	add	sp, sp, #0x1e0
1018b52a4:	c0 03 5f d6	ret
1018b52a8:	f3 03 00 aa	mov	x19, x0
1018b52ac:	e0 23 00 91	add	x0, sp, #0x8
1018b52b0:	68 a3 a5 97	bl	0x10021e050
1018b52b4:	e0 03 13 aa	mov	x0, x19
1018b52b8:	71 99 6e 94	bl	0x10345b87c ; symbol stub for: __Unwind_Resume
1018b52bc:	f3 03 00 aa	mov	x19, x0
1018b52c0:	e0 23 00 91	add	x0, sp, #0x8
1018b52c4:	63 a3 a5 97	bl	0x10021e050
1018b52c8:	e0 03 13 aa	mov	x0, x19
1018b52cc:	6c 99 6e 94	bl	0x10345b87c ; symbol stub for: __Unwind_Resume
1018b52d0:	ff 83 01 d1	sub	sp, sp, #0x60
1018b52d4:	f4 4f 04 a9	stp	x20, x19, [sp, #0x40]
1018b52d8:	fd 7b 05 a9	stp	x29, x30, [sp, #0x50]
1018b52dc:	fd 43 01 91	add	x29, sp, #0x50
1018b52e0:	f3 03 00 aa	mov	x19, x0
1018b52e4:	08 20 44 39	ldrb	w8, [x0, #0x108]
1018b52e8:	68 01 00 34	cbz	w8, 0x1018b5314
1018b52ec:	68 1e c4 39	ldrsb	w8, [x19, #0x107]
1018b52f0:	48 03 f8 37	tbnz	w8, #0x1f, 0x1018b5358
1018b52f4:	68 c2 03 91	add	x8, x19, #0xf0
1018b52f8:	00 01 c0 3d	ldr	q0, [x8]
1018b52fc:	e0 0b 80 3d	str	q0, [sp, #0x20]
1028d7780:	c0 03 5f d6	ret
1028d7784:	01 1c 00 b9	str	w1, [x0, #0x1c]
1028d7788:	c0 03 5f d6	ret
1028d778c:	08 1c 40 b9	ldr	w8, [x0, #0x1c]
