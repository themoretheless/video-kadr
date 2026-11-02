__ZN9arvrender13FlowstateStab15CalFlowstateMapEN3ins8IdxTimedERKNSt3__112basic_stringIcNS3_11char_traitsIcEENS3_9allocatorIcEEEE:
101081514:	e9 23 b9 6d	stp	d9, d8, [sp, #-0x70]!
101081518:	fc 6f 01 a9	stp	x28, x27, [sp, #0x10]
10108151c:	fa 67 02 a9	stp	x26, x25, [sp, #0x20]
101081520:	f8 5f 03 a9	stp	x24, x23, [sp, #0x30]
101081524:	f6 57 04 a9	stp	x22, x21, [sp, #0x40]
101081528:	f4 4f 05 a9	stp	x20, x19, [sp, #0x50]
10108152c:	fd 7b 06 a9	stp	x29, x30, [sp, #0x60]
101081530:	fd 83 01 91	add	x29, sp, #0x60
101081534:	ff 83 0a d1	sub	sp, sp, #0x2a0
101081538:	f5 03 01 aa	mov	x21, x1
10108153c:	f3 03 00 aa	mov	x19, x0
101081540:	e0 03 02 aa	mov	x0, x2
101081544:	56 d2 20 94	bl	__ZN9arvrender10OffsetUtil12GetLensCountERKNSt3__112basic_stringIcNS1_11char_traitsIcEENS1_9allocatorIcEEEE
101081548:	68 12 40 f9	ldr	x8, [x19, #0x20]
10108154c:	a8 43 00 b4	cbz	x8, 0x101081dc0
101081550:	f6 03 00 aa	mov	x22, x0
101081554:	68 1a 40 f9	ldr	x8, [x19, #0x30]
101081558:	1f 01 00 f1	cmp	x8, #0x0
10108155c:	04 18 40 7a	ccmp	w0, #0x0, #0x4, ne
101081560:	00 43 00 54	b.eq	0x101081dc0
101081564:	5d 83 8f 94	bl	0x1034622d8 ; symbol stub for: __ZNSt3__16chrono12steady_clock3nowEv
101081568:	f4 03 00 aa	mov	x20, x0
10108156c:	68 1a 40 f9	ldr	x8, [x19, #0x30]
101081570:	09 09 40 f9	ldr	x9, [x8, #0x10]
101081574:	0a 21 41 69	ldpsw	x10, x8, [x8, #0x8]
101081578:	a9 ab 37 a9	stp	x9, x10, [x29, #-0x88]
10108157c:	a8 a3 38 a9	stp	x8, x8, [x29, #-0x78]
101081580:	60 0a 40 f9	ldr	x0, [x19, #0x10]
101081584:	a8 1a 40 f9	ldr	x8, [x21, #0x30]
101081588:	a8 03 17 f8	stur	x8, [x29, #-0x90]
10108158c:	a2 0a c0 3d	ldr	q2, [x21, #0x20]
101081590:	a1 02 40 ad	ldp	q1, q0, [x21]
101081594:	a0 8b 3a ad	stp	q0, q2, [x29, #-0xb0]
101081598:	a1 03 94 3c	stur	q1, [x29, #-0xc0]
10108159c:	08 00 40 f9	ldr	x8, [x0]
1010815a0:	08 25 40 f9	ldr	x8, [x8, #0x48]
1010815a4:	a1 03 03 d1	sub	x1, x29, #0xc0
1010815a8:	02 00 80 52	mov	w2, #0x0
1010815ac:	00 01 3f d6	blr	x8
1010815b0:	08 40 60 1e	fmov	d8, d0
1010815b4:	bf ff 32 a9	stp	xzr, xzr, [x29, #-0xd8]
1010815b8:	bf 83 13 f8	stur	xzr, [x29, #-0xc8]
1010815bc:	bf 7f 31 a9	stp	xzr, xzr, [x29, #-0xf0]
1010815c0:	bf 03 12 f8	stur	xzr, [x29, #-0xe0]
1010815c4:	00 01 80 52	mov	w0, #0x8
1010815c8:	16 84 8f 94	bl	0x103462620 ; symbol stub for: __Znwm
1010815cc:	88 3f 95 d2	mov	x8, #0xa9fc
1010815d0:	28 5e ba f2	movk	x8, #0xd2f1, lsl #16
1010815d4:	a8 49 cc f2	movk	x8, #0x624d, lsl #32
1010815d8:	08 ea e7 f2	movk	x8, #0x3f50, lsl #48
1010815dc:	00 01 67 9e	fmov	d0, x8
1010815e0:	00 09 60 1e	fmul	d0, d8, d0
1010815e4:	e8 03 00 aa	mov	x8, x0
1010815e8:	00 85 00 fc	str	d0, [x8], #0x8
1010815ec:	a8 23 33 a9	stp	x8, x8, [x29, #-0xd0]
1010815f0:	a0 83 12 f8	stur	x0, [x29, #-0xd8]
1010815f4:	df 06 00 71	cmp	w22, #0x1
1010815f8:	61 03 00 54	b.ne	0x101081664
1010815fc:	60 0a 40 f9	ldr	x0, [x19, #0x10]
101081600:	a0 06 40 ad	ldp	q0, q1, [x21]
101081604:	e0 87 0e ad	stp	q0, q1, [sp, #0x1d0]
101081608:	a0 0a c0 3d	ldr	q0, [x21, #0x20]
10108160c:	e0 7f 80 3d	str	q0, [sp, #0x1f0]
101081610:	a8 1a 40 f9	ldr	x8, [x21, #0x30]
101081614:	e8 03 01 f9	str	x8, [sp, #0x200]
101081618:	08 00 40 f9	ldr	x8, [x0]
10108161c:	08 55 40 f9	ldr	x8, [x8, #0xa8]
101081620:	e1 43 07 91	add	x1, sp, #0x1d0
101081624:	22 00 80 52	mov	w2, #0x1
101081628:	00 01 3f d6	blr	x8
10108162c:	77 12 40 f9	ldr	x23, [x19, #0x20]
101081630:	00 e4 00 2f	movi	d0, #0000000000000000
101081634:	60 01 00 37	tbnz	w0, #0x0, 0x101081660
101081638:	60 0a 40 f9	ldr	x0, [x19, #0x10]
10108163c:	08 00 40 f9	ldr	x8, [x0]
101081640:	08 51 40 f9	ldr	x8, [x8, #0xa0]
101081644:	00 01 3f d6	blr	x8
101081648:	88 3f 95 d2	mov	x8, #0xa9fc
10108164c:	28 5e ba f2	movk	x8, #0xd2f1, lsl #16
101081650:	a8 49 cc f2	movk	x8, #0x624d, lsl #32
101081654:	08 ea e7 f2	movk	x8, #0x3f50, lsl #48
101081658:	01 01 67 9e	fmov	d1, x8
10108165c:	00 08 61 1e	fmul	d0, d0, d1
101081660:	e0 56 00 fd	str	d0, [x23, #0xa8]
101081664:	b9 a3 71 a9	ldp	x25, x8, [x29, #-0xe8]
101081668:	3f 03 08 eb	cmp	x25, x8
10108166c:	a2 00 00 54	b.hs	0x101081680
101081670:	28 00 80 52	mov	w8, #0x1
101081674:	28 17 00 38	strb	w8, [x25], #0x1
101081678:	f8 03 19 aa	mov	x24, x25
10108167c:	52 00 00 14	b	0x1010817c4
101081680:	b7 03 51 f8	ldur	x23, [x29, #-0xf0]
101081684:	3a 03 17 cb	sub	x26, x25, x23
101081688:	49 07 00 b1	adds	x9, x26, #0x1
10108168c:	84 3c 00 54	b.mi	0x101081e1c
101081690:	0a 00 f8 92	mov	x10, #0x3fffffffffffffff
101081694:	08 01 17 cb	sub	x8, x8, x23
101081698:	0b f9 7f d3	lsl	x11, x8, #1
10108169c:	7f 01 09 eb	cmp	x11, x9
1010816a0:	69 81 89 9a	csel	x9, x11, x9, hi
1010816a4:	1f 01 0a eb	cmp	x8, x10
1010816a8:	08 00 f0 92	mov	x8, #0x7fffffffffffffff
1010816ac:	38 31 88 9a	csel	x24, x9, x8, lo
1010816b0:	98 00 00 b4	cbz	x24, 0x1010816c0
1010816b4:	e0 03 18 aa	mov	x0, x24
1010816b8:	da 83 8f 94	bl	0x103462620 ; symbol stub for: __Znwm
1010816bc:	02 00 00 14	b	0x1010816c4
1010816c0:	00 00 80 d2	mov	x0, #0x0
1010816c4:	09 00 1a 8b	add	x9, x0, x26
1010816c8:	08 00 18 8b	add	x8, x0, x24
1010816cc:	2a 00 80 52	mov	w10, #0x1
1010816d0:	f8 03 09 aa	mov	x24, x9
1010816d4:	0a 17 00 38	strb	w10, [x24], #0x1
1010816d8:	3f 03 17 eb	cmp	x25, x23
1010816dc:	a0 06 00 54	b.eq	0x1010817b0
1010816e0:	5f 23 00 f1	cmp	x26, #0x8
1010816e4:	a3 05 00 54	b.lo	0x101081798
1010816e8:	0a 00 19 8b	add	x10, x0, x25
1010816ec:	ea 02 0a cb	sub	x10, x23, x10
1010816f0:	4a 01 19 8b	add	x10, x10, x25
1010816f4:	5f 01 01 f1	cmp	x10, #0x40
1010816f8:	03 05 00 54	b.lo	0x101081798
1010816fc:	5f 03 01 f1	cmp	x26, #0x40
101081700:	62 00 00 54	b.hs	0x10108170c
101081704:	0a 00 80 d2	mov	x10, #0x0
101081708:	12 00 00 14	b	0x101081750
10108170c:	4a e7 7a 92	and	x10, x26, #0xffffffffffffffc0
101081710:	2b 83 00 d1	sub	x11, x25, #0x20
101081714:	6c 01 17 cb	sub	x12, x11, x23
101081718:	0c 00 0c 8b	add	x12, x0, x12
10108171c:	ed 03 0a aa	mov	x13, x10
101081720:	61 01 40 ad	ldp	q1, q0, [x11]
101081724:	63 09 7f ad	ldp	q3, q2, [x11, #-0x20]
101081728:	81 01 00 ad	stp	q1, q0, [x12]
10108172c:	83 09 3f ad	stp	q3, q2, [x12, #-0x20]
101081730:	8c 01 01 d1	sub	x12, x12, #0x40
101081734:	6b 01 01 d1	sub	x11, x11, #0x40
101081738:	ad 01 01 d1	sub	x13, x13, #0x40
10108173c:	2d ff ff b5	cbnz	x13, 0x101081720
101081740:	5f 03 0a eb	cmp	x26, x10
101081744:	40 03 00 54	b.eq	0x1010817ac
101081748:	5f 0b 7d f2	tst	x26, #0x38
10108174c:	20 02 00 54	b.eq	0x101081790
101081750:	4b f3 7d 92	and	x11, x26, #0xfffffffffffffff8
101081754:	2c 03 0b cb	sub	x12, x25, x11
101081758:	29 01 0b cb	sub	x9, x9, x11
10108175c:	2d 03 0a cb	sub	x13, x25, x10
101081760:	ad 21 00 d1	sub	x13, x13, #0x8
101081764:	ae 01 17 cb	sub	x14, x13, x23
101081768:	4a 01 0b cb	sub	x10, x10, x11
10108176c:	a0 85 5f fc	ldr	d0, [x13], #-0x8
101081770:	00 68 2e fc	str	d0, [x0, x14]
101081774:	ce 21 00 d1	sub	x14, x14, #0x8
101081778:	4a 21 00 b1	adds	x10, x10, #0x8
10108177c:	81 ff ff 54	b.ne	0x10108176c
101081780:	f9 03 0c aa	mov	x25, x12
101081784:	5f 03 0b eb	cmp	x26, x11
101081788:	81 00 00 54	b.ne	0x101081798
10108178c:	08 00 00 14	b	0x1010817ac
101081790:	29 01 0a cb	sub	x9, x9, x10
101081794:	39 03 0a cb	sub	x25, x25, x10
101081798:	29 05 00 d1	sub	x9, x9, #0x1
10108179c:	2a ff 5f 38	ldrb	w10, [x25, #-0x1]!
1010817a0:	2a f5 1f 38	strb	w10, [x9], #-0x1
1010817a4:	3f 03 17 eb	cmp	x25, x23
1010817a8:	a1 ff ff 54	b.ne	0x10108179c
1010817ac:	e9 03 00 aa	mov	x9, x0
1010817b0:	a9 63 31 a9	stp	x9, x24, [x29, #-0xf0]
1010817b4:	a8 03 12 f8	stur	x8, [x29, #-0xe0]
1010817b8:	77 00 00 b4	cbz	x23, 0x1010817c4
1010817bc:	e0 03 17 aa	mov	x0, x23
1010817c0:	56 83 8f 94	bl	0x103462518 ; symbol stub for: __ZdlPv
1010817c4:	b8 83 11 f8	stur	x24, [x29, #-0xe8]
1010817c8:	60 0a 40 f9	ldr	x0, [x19, #0x10]
1010817cc:	df 0a 00 71	cmp	w22, #0x2
1010817d0:	21 17 00 54	b.ne	0x101081ab4
1010817d4:	a0 06 40 ad	ldp	q0, q1, [x21]
1010817d8:	e0 87 0c ad	stp	q0, q1, [sp, #0x190]
1010817dc:	a0 0a c0 3d	ldr	q0, [x21, #0x20]
1010817e0:	e0 6f 80 3d	str	q0, [sp, #0x1b0]
1010817e4:	a8 1a 40 f9	ldr	x8, [x21, #0x30]
1010817e8:	e8 e3 00 f9	str	x8, [sp, #0x1c0]
1010817ec:	08 00 40 f9	ldr	x8, [x0]
1010817f0:	08 25 40 f9	ldr	x8, [x8, #0x48]
1010817f4:	e1 43 06 91	add	x1, sp, #0x190
1010817f8:	22 00 80 52	mov	w2, #0x1
1010817fc:	00 01 3f d6	blr	x8
101081800:	88 3f 95 d2	mov	x8, #0xa9fc
101081804:	28 5e ba f2	movk	x8, #0xd2f1, lsl #16
101081808:	a8 49 cc f2	movk	x8, #0x624d, lsl #32
10108180c:	08 ea e7 f2	movk	x8, #0x3f50, lsl #48
101081810:	01 01 67 9e	fmov	d1, x8
101081814:	08 08 61 1e	fmul	d8, d0, d1
101081818:	b6 23 73 a9	ldp	x22, x8, [x29, #-0xd0]
10108181c:	df 02 08 eb	cmp	x22, x8
101081820:	82 00 00 54	b.hs	0x101081830
101081824:	c8 86 00 fc	str	d8, [x22], #0x8
101081828:	f7 03 16 aa	mov	x23, x22
10108182c:	47 00 00 14	b	0x101081948
101081830:	b5 83 52 f8	ldur	x21, [x29, #-0xd8]
101081834:	d9 02 15 cb	sub	x25, x22, x21
101081838:	38 ff 43 93	asr	x24, x25, #3
10108183c:	09 07 00 91	add	x9, x24, #0x1
101081840:	2a fd 7d d3	lsr	x10, x9, #61
101081844:	ea 2f 00 b5	cbnz	x10, 0x101081e40
101081848:	ea ef 7d b2	mov	x10, #0x7ffffffffffffff8
10108184c:	08 01 15 cb	sub	x8, x8, x21
101081850:	0b fd 42 93	asr	x11, x8, #2
101081854:	7f 01 09 eb	cmp	x11, x9
101081858:	69 81 89 9a	csel	x9, x11, x9, hi
10108185c:	1f 01 0a eb	cmp	x8, x10
101081860:	08 00 fc 92	mov	x8, #0x1fffffffffffffff
101081864:	37 31 88 9a	csel	x23, x9, x8, lo
101081868:	97 01 00 b4	cbz	x23, 0x101081898
10108186c:	e8 fe 7d d3	lsr	x8, x23, #61
101081870:	48 2f 00 b5	cbnz	x8, 0x101081e58
101081874:	e0 f2 7d d3	lsl	x0, x23, #3
101081878:	6a 83 8f 94	bl	0x103462620 ; symbol stub for: __Znwm
10108187c:	09 0c 18 8b	add	x9, x0, x24, lsl #3
101081880:	08 0c 17 8b	add	x8, x0, x23, lsl #3
101081884:	f7 03 09 aa	mov	x23, x9
101081888:	e8 86 00 fc	str	d8, [x23], #0x8
10108188c:	ca 02 15 eb	subs	x10, x22, x21
101081890:	21 01 00 54	b.ne	0x1010818b4
101081894:	28 00 00 14	b	0x101081934
101081898:	00 00 80 d2	mov	x0, #0x0
10108189c:	09 0c 18 8b	add	x9, x0, x24, lsl #3
1010818a0:	08 0c 17 8b	add	x8, x0, x23, lsl #3
1010818a4:	f7 03 09 aa	mov	x23, x9
1010818a8:	e8 86 00 fc	str	d8, [x23], #0x8
1010818ac:	ca 02 15 eb	subs	x10, x22, x21
1010818b0:	20 04 00 54	b.eq	0x101081934
1010818b4:	4a 21 00 d1	sub	x10, x10, #0x8
1010818b8:	5f 61 01 f1	cmp	x10, #0x58
1010818bc:	43 03 00 54	b.lo	0x101081924
1010818c0:	2b 03 00 8b	add	x11, x25, x0
1010818c4:	cb 02 0b cb	sub	x11, x22, x11
1010818c8:	7f 01 01 f1	cmp	x11, #0x40
1010818cc:	c3 02 00 54	b.lo	0x101081924
1010818d0:	4a fd 43 d3	lsr	x10, x10, #3
1010818d4:	4a 05 00 91	add	x10, x10, #0x1
1010818d8:	4b e9 7d 92	and	x11, x10, #0x3ffffffffffffff8
1010818dc:	6d f1 7d d3	lsl	x13, x11, #3
1010818e0:	cc 02 0d cb	sub	x12, x22, x13
1010818e4:	29 01 0d cb	sub	x9, x9, x13
1010818e8:	cd 82 00 d1	sub	x13, x22, #0x20
1010818ec:	0e 0c 18 8b	add	x14, x0, x24, lsl #3
1010818f0:	ce 81 00 d1	sub	x14, x14, #0x20
1010818f4:	ef 03 0b aa	mov	x15, x11
1010818f8:	a1 01 40 ad	ldp	q1, q0, [x13]
1010818fc:	a3 09 7f ad	ldp	q3, q2, [x13, #-0x20]
101081900:	c1 01 00 ad	stp	q1, q0, [x14]
101081904:	c3 09 3f ad	stp	q3, q2, [x14, #-0x20]
101081908:	ad 01 01 d1	sub	x13, x13, #0x40
10108190c:	ce 01 01 d1	sub	x14, x14, #0x40
101081910:	ef 21 00 d1	sub	x15, x15, #0x8
101081914:	2f ff ff b5	cbnz	x15, 0x1010818f8
101081918:	f6 03 0c aa	mov	x22, x12
10108191c:	5f 01 0b eb	cmp	x10, x11
101081920:	a0 00 00 54	b.eq	0x101081934
101081924:	c0 8e 5f fc	ldr	d0, [x22, #-0x8]!
101081928:	20 8d 1f fc	str	d0, [x9, #-0x8]!
10108192c:	df 02 15 eb	cmp	x22, x21
101081930:	a1 ff ff 54	b.ne	0x101081924
101081934:	a9 df 32 a9	stp	x9, x23, [x29, #-0xd8]
101081938:	a8 83 13 f8	stur	x8, [x29, #-0xc8]
10108193c:	75 00 00 b4	cbz	x21, 0x101081948
101081940:	e0 03 15 aa	mov	x0, x21
101081944:	f5 82 8f 94	bl	0x103462518 ; symbol stub for: __ZdlPv
101081948:	b7 03 13 f8	stur	x23, [x29, #-0xd0]
10108194c:	b7 a3 71 a9	ldp	x23, x8, [x29, #-0xe8]
101081950:	ff 02 08 eb	cmp	x23, x8
101081954:	a2 00 00 54	b.hs	0x101081968
101081958:	28 00 80 52	mov	w8, #0x1
10108195c:	e8 16 00 38	strb	w8, [x23], #0x1
101081960:	f6 03 17 aa	mov	x22, x23
101081964:	52 00 00 14	b	0x101081aac
101081968:	b5 03 51 f8	ldur	x21, [x29, #-0xf0]
10108196c:	f8 02 15 cb	sub	x24, x23, x21
101081970:	09 07 00 b1	adds	x9, x24, #0x1
101081974:	c4 26 00 54	b.mi	0x101081e4c
101081978:	0a 00 f8 92	mov	x10, #0x3fffffffffffffff
10108197c:	08 01 15 cb	sub	x8, x8, x21
101081980:	0b f9 7f d3	lsl	x11, x8, #1
101081984:	7f 01 09 eb	cmp	x11, x9
101081988:	69 81 89 9a	csel	x9, x11, x9, hi
10108198c:	1f 01 0a eb	cmp	x8, x10
101081990:	08 00 f0 92	mov	x8, #0x7fffffffffffffff
101081994:	36 31 88 9a	csel	x22, x9, x8, lo
101081998:	96 00 00 b4	cbz	x22, 0x1010819a8
10108199c:	e0 03 16 aa	mov	x0, x22
1010819a0:	20 83 8f 94	bl	0x103462620 ; symbol stub for: __Znwm
1010819a4:	02 00 00 14	b	0x1010819ac
1010819a8:	00 00 80 d2	mov	x0, #0x0
1010819ac:	09 00 18 8b	add	x9, x0, x24
1010819b0:	08 00 16 8b	add	x8, x0, x22
1010819b4:	2a 00 80 52	mov	w10, #0x1
1010819b8:	f6 03 09 aa	mov	x22, x9
1010819bc:	ca 16 00 38	strb	w10, [x22], #0x1
1010819c0:	ff 02 15 eb	cmp	x23, x21
1010819c4:	a0 06 00 54	b.eq	0x101081a98
1010819c8:	1f 23 00 f1	cmp	x24, #0x8
1010819cc:	a3 05 00 54	b.lo	0x101081a80
1010819d0:	0a 00 17 8b	add	x10, x0, x23
1010819d4:	aa 02 0a cb	sub	x10, x21, x10
1010819d8:	4a 01 17 8b	add	x10, x10, x23
1010819dc:	5f 01 01 f1	cmp	x10, #0x40
1010819e0:	03 05 00 54	b.lo	0x101081a80
1010819e4:	1f 03 01 f1	cmp	x24, #0x40
1010819e8:	62 00 00 54	b.hs	0x1010819f4
1010819ec:	0a 00 80 d2	mov	x10, #0x0
1010819f0:	12 00 00 14	b	0x101081a38
1010819f4:	0a e7 7a 92	and	x10, x24, #0xffffffffffffffc0
1010819f8:	eb 82 00 d1	sub	x11, x23, #0x20
1010819fc:	6c 01 15 cb	sub	x12, x11, x21
101081a00:	0c 00 0c 8b	add	x12, x0, x12
101081a04:	ed 03 0a aa	mov	x13, x10
101081a08:	61 01 40 ad	ldp	q1, q0, [x11]
101081a0c:	63 09 7f ad	ldp	q3, q2, [x11, #-0x20]
101081a10:	81 01 00 ad	stp	q1, q0, [x12]
101081a14:	83 09 3f ad	stp	q3, q2, [x12, #-0x20]
101081a18:	8c 01 01 d1	sub	x12, x12, #0x40
101081a1c:	6b 01 01 d1	sub	x11, x11, #0x40
101081a20:	ad 01 01 d1	sub	x13, x13, #0x40
101081a24:	2d ff ff b5	cbnz	x13, 0x101081a08
101081a28:	1f 03 0a eb	cmp	x24, x10
101081a2c:	40 03 00 54	b.eq	0x101081a94
101081a30:	1f 0b 7d f2	tst	x24, #0x38
101081a34:	20 02 00 54	b.eq	0x101081a78
101081a38:	0b f3 7d 92	and	x11, x24, #0xfffffffffffffff8
101081a3c:	ec 02 0b cb	sub	x12, x23, x11
101081a40:	29 01 0b cb	sub	x9, x9, x11
101081a44:	ed 02 0a cb	sub	x13, x23, x10
101081a48:	ad 21 00 d1	sub	x13, x13, #0x8
101081a4c:	ae 01 15 cb	sub	x14, x13, x21
101081a50:	4a 01 0b cb	sub	x10, x10, x11
101081a54:	a0 85 5f fc	ldr	d0, [x13], #-0x8
101081a58:	00 68 2e fc	str	d0, [x0, x14]
101081a5c:	ce 21 00 d1	sub	x14, x14, #0x8
101081a60:	4a 21 00 b1	adds	x10, x10, #0x8
101081a64:	81 ff ff 54	b.ne	0x101081a54
101081a68:	f7 03 0c aa	mov	x23, x12
101081a6c:	1f 03 0b eb	cmp	x24, x11
101081a70:	81 00 00 54	b.ne	0x101081a80
101081a74:	08 00 00 14	b	0x101081a94
101081a78:	29 01 0a cb	sub	x9, x9, x10
101081a7c:	f7 02 0a cb	sub	x23, x23, x10
101081a80:	29 05 00 d1	sub	x9, x9, #0x1
101081a84:	ea fe 5f 38	ldrb	w10, [x23, #-0x1]!
101081a88:	2a f5 1f 38	strb	w10, [x9], #-0x1
101081a8c:	ff 02 15 eb	cmp	x23, x21
101081a90:	a1 ff ff 54	b.ne	0x101081a84
101081a94:	e9 03 00 aa	mov	x9, x0
101081a98:	a9 5b 31 a9	stp	x9, x22, [x29, #-0xf0]
101081a9c:	a8 03 12 f8	stur	x8, [x29, #-0xe0]
101081aa0:	75 00 00 b4	cbz	x21, 0x101081aac
101081aa4:	e0 03 15 aa	mov	x0, x21
101081aa8:	9c 82 8f 94	bl	0x103462518 ; symbol stub for: __ZdlPv
101081aac:	b6 83 11 f8	stur	x22, [x29, #-0xe8]
101081ab0:	60 0a 40 f9	ldr	x0, [x19, #0x10]
101081ab4:	08 00 40 f9	ldr	x8, [x0]
101081ab8:	08 3d 40 f9	ldr	x8, [x8, #0x78]
101081abc:	00 01 3f d6	blr	x8
101081ac0:	40 06 00 34	cbz	w0, 0x101081b88
101081ac4:	75 12 40 f9	ldr	x21, [x19, #0x20]
101081ac8:	ff 7f 18 a9	stp	xzr, xzr, [sp, #0x180]
101081acc:	ff bf 00 f9	str	xzr, [sp, #0x178]
101081ad0:	b6 a3 72 a9	ldp	x22, x8, [x29, #-0xd8]
101081ad4:	17 01 16 eb	subs	x23, x8, x22
101081ad8:	80 01 00 54	b.eq	0x101081b08
101081adc:	77 1a f8 b7	tbnz	x23, #0x3f, 0x101081e28
101081ae0:	e0 03 17 aa	mov	x0, x23
101081ae4:	cf 82 8f 94	bl	0x103462620 ; symbol stub for: __Znwm
101081ae8:	e8 fe 43 93	asr	x8, x23, #3
101081aec:	e0 83 17 a9	stp	x0, x0, [sp, #0x178]
101081af0:	18 0c 08 8b	add	x24, x0, x8, lsl #3
101081af4:	f8 c7 00 f9	str	x24, [sp, #0x188]
101081af8:	e1 03 16 aa	mov	x1, x22
101081afc:	e2 03 17 aa	mov	x2, x23
101081b00:	4c 86 8f 94	bl	0x103463430 ; symbol stub for: _memcpy
101081b04:	f8 c3 00 f9	str	x24, [sp, #0x180]
101081b08:	ff 7f 16 a9	stp	xzr, xzr, [sp, #0x160]
101081b0c:	ff bb 00 f9	str	xzr, [sp, #0x170]
101081b10:	b6 23 71 a9	ldp	x22, x8, [x29, #-0xf0]
101081b14:	17 01 16 eb	subs	x23, x8, x22
101081b18:	60 01 00 54	b.eq	0x101081b44
101081b1c:	d7 18 f8 b7	tbnz	x23, #0x3f, 0x101081e34
101081b20:	e0 03 17 aa	mov	x0, x23
101081b24:	bf 82 8f 94	bl	0x103462620 ; symbol stub for: __Znwm
101081b28:	e0 03 16 a9	stp	x0, x0, [sp, #0x160]
101081b2c:	18 00 17 8b	add	x24, x0, x23
101081b30:	f8 bb 00 f9	str	x24, [sp, #0x170]
101081b34:	e1 03 16 aa	mov	x1, x22
101081b38:	e2 03 17 aa	mov	x2, x23
101081b3c:	3d 86 8f 94	bl	0x103463430 ; symbol stub for: _memcpy
101081b40:	f8 b7 00 f9	str	x24, [sp, #0x168]
101081b44:	e8 43 01 91	add	x8, sp, #0x50
101081b48:	e1 e3 05 91	add	x1, sp, #0x178
101081b4c:	e2 83 05 91	add	x2, sp, #0x160
101081b50:	a3 23 02 d1	sub	x3, x29, #0x88
101081b54:	e0 03 15 aa	mov	x0, x21
101081b58:	e1 50 2b 94	bl	__ZN9flowstate20MultiFlowstateSystem31getEquirectangleFlowMapMultiIMUENSt3__16vectorIdNS1_9allocatorIdEEEENS2_IN3ins9SweepModeENS3_IS7_EEEERN5Eigen3RefINSA_6MatrixINSC_IfLi2ELi1ELi0ELi2ELi1EEELin1ELin1ELi1ELin1ELin1EEELi0ENSA_11OuterStrideILin1EEEEE
101081b5c:	e1 83 42 ad	ldp	q1, q0, [sp, #0x50]
101081b60:	e0 07 00 ad	stp	q0, q1, [sp]
101081b64:	e0 b3 40 f9	ldr	x0, [sp, #0x160]
101081b68:	60 00 00 b4	cbz	x0, 0x101081b74
101081b6c:	e0 b7 00 f9	str	x0, [sp, #0x168]
101081b70:	6a 82 8f 94	bl	0x103462518 ; symbol stub for: __ZdlPv
101081b74:	e0 bf 40 f9	ldr	x0, [sp, #0x178]
101081b78:	e0 03 00 b4	cbz	x0, 0x101081bf4
101081b7c:	e0 c3 00 f9	str	x0, [sp, #0x180]
101081b80:	66 82 8f 94	bl	0x103462518 ; symbol stub for: __ZdlPv
101081b84:	1c 00 00 14	b	0x101081bf4
101081b88:	60 0a 40 f9	ldr	x0, [x19, #0x10]
101081b8c:	08 00 40 f9	ldr	x8, [x0]
101081b90:	09 4d 40 f9	ldr	x9, [x8, #0x98]
101081b94:	e8 43 01 91	add	x8, sp, #0x50
101081b98:	20 01 3f d6	blr	x9
101081b9c:	f6 57 45 a9	ldp	x22, x21, [sp, #0x50]
101081ba0:	75 01 00 b4	cbz	x21, 0x101081bcc
101081ba4:	a8 22 00 91	add	x8, x21, #0x8
101081ba8:	09 00 80 92	mov	x9, #-0x1
101081bac:		.long	0xf8e90108
101081bb0:	e8 00 00 b5	cbnz	x8, 0x101081bcc
101081bb4:	a8 02 40 f9	ldr	x8, [x21]
101081bb8:	08 09 40 f9	ldr	x8, [x8, #0x10]
101081bbc:	e0 03 15 aa	mov	x0, x21
101081bc0:	00 01 3f d6	blr	x8
101081bc4:	e0 03 15 aa	mov	x0, x21
101081bc8:	73 81 8f 94	bl	0x103462194 ; symbol stub for: __ZNSt3__119__shared_weak_count14__release_weakEv
101081bcc:	56 01 00 b4	cbz	x22, 0x101081bf4
101081bd0:	60 12 40 f9	ldr	x0, [x19, #0x20]
101081bd4:	e8 43 01 91	add	x8, sp, #0x50
101081bd8:	a1 63 03 d1	sub	x1, x29, #0xd8
101081bdc:	a2 c3 03 d1	sub	x2, x29, #0xf0
101081be0:	a3 23 02 d1	sub	x3, x29, #0x88
101081be4:	e8 4f 2b 94	bl	__ZN9flowstate20MultiFlowstateSystem23getEquirectangleFlowMapERKNSt3__16vectorIdNS1_9allocatorIdEEEERKNS2_IN3ins9SweepModeENS3_IS9_EEEERN5Eigen3RefINSE_6MatrixINSG_IfLi2ELi1ELi0ELi2ELi1EEELin1ELin1ELi1ELin1ELin1EEELi0ENSE_11OuterStrideILin1EEEEE
101081be8:	e1 83 42 ad	ldp	q1, q0, [sp, #0x50]
101081bec:	e0 07 00 ad	stp	q0, q1, [sp]
101081bf0:	01 00 00 14	b	0x101081bf4
101081bf4:	f5 64 01 b0	adrp	x21, 11421 ; 0x103d1e000
101081bf8:	b5 fe 14 91	add	x21, x21, #0x53f ; literal pool for: "/Users/insta360/jenkins/workspace/Studio_Mac/build/studio_src/deps/fcp/grv/ARVRender/FrameData/FlowstateStab.cc"
101081bfc:	e0 03 15 aa	mov	x0, x21
101081c00:	e1 05 80 52	mov	w1, #0x2f
101081c04:	eb 87 8f 94	bl	0x103463bb0 ; symbol stub for: _strrchr
101081c08:	1f 00 00 f1	cmp	x0, #0x0
101081c0c:	b6 06 80 9a	csinc	x22, x21, x0, eq
101081c10:	e0 03 16 aa	mov	x0, x22
101081c14:	d2 87 8f 94	bl	0x103463b5c ; symbol stub for: _strlen
101081c18:	e8 ef 7d b2	mov	x8, #0x7ffffffffffffff8
101081c1c:	1f 00 08 eb	cmp	x0, x8
101081c20:	82 0f 00 54	b.hs	0x101081e10
101081c24:	f5 03 00 aa	mov	x21, x0
101081c28:	1f 5c 00 f1	cmp	x0, #0x17
101081c2c:	a2 00 00 54	b.hs	0x101081c40
101081c30:	f5 3f 01 39	strb	w21, [sp, #0x4f]
101081c34:	f7 e3 00 91	add	x23, sp, #0x38
101081c38:	d5 01 00 b5	cbnz	x21, 0x101081c70
101081c3c:	11 00 00 14	b	0x101081c80
101081c40:	a8 f2 7d 92	and	x8, x21, #0xfffffffffffffff8
101081c44:	08 21 00 91	add	x8, x8, #0x8
101081c48:	a9 0a 40 b2	orr	x9, x21, #0x7
101081c4c:	3f 5d 00 f1	cmp	x9, #0x17
101081c50:	08 01 89 9a	csel	x8, x8, x9, eq
101081c54:	18 05 00 91	add	x24, x8, #0x1
101081c58:	e0 03 18 aa	mov	x0, x24
101081c5c:	71 82 8f 94	bl	0x103462620 ; symbol stub for: __Znwm
101081c60:	f7 03 00 aa	mov	x23, x0
101081c64:	08 03 41 b2	orr	x8, x24, #0x8000000000000000
101081c68:	f5 23 04 a9	stp	x21, x8, [sp, #0x40]
101081c6c:	e0 1f 00 f9	str	x0, [sp, #0x38]
101081c70:	e0 03 17 aa	mov	x0, x23
101081c74:	e1 03 16 aa	mov	x1, x22
101081c78:	e2 03 15 aa	mov	x2, x21
101081c7c:	f0 85 8f 94	bl	0x10346343c ; symbol stub for: _memmove
101081c80:	ff 6a 35 38	strb	wzr, [x23, x21]
101081c84:	e8 01 80 52	mov	w8, #0xf
101081c88:	e9 64 01 b0	adrp	x9, 11421 ; 0x103d1e000
101081c8c:	29 3d 19 91	add	x9, x9, #0x64f ; literal pool for: "CalFlowstateMap"
101081c90:	e8 df 00 39	strb	w8, [sp, #0x37]
101081c94:	28 01 40 f9	ldr	x8, [x9]
101081c98:	e8 13 00 f9	str	x8, [sp, #0x20]
101081c9c:	28 71 40 f8	ldur	x8, [x9, #0x7]
101081ca0:	e8 73 02 f8	stur	x8, [sp, #0x27]
101081ca4:	ff bf 00 39	strb	wzr, [sp, #0x2f]
101081ca8:	e0 43 01 91	add	x0, sp, #0x50
101081cac:	e1 e3 00 91	add	x1, sp, #0x38
101081cb0:	e2 83 00 91	add	x2, sp, #0x20
101081cb4:	43 1a 80 52	mov	w3, #0xd2
101081cb8:	24 00 80 52	mov	w4, #0x1
101081cbc:	f9 a6 be 97	bl	__ZN3ins11LLogMessageC1ERKNSt3__112basic_stringIcNS1_11char_traitsIcEENS1_9allocatorIcEEEES9_ii
101081cc0:	e1 64 01 b0	adrp	x1, 11421 ; 0x103d1e000
101081cc4:	21 7c 19 91	add	x1, x1, #0x65f ; literal pool for: "[stitch]    until get flowsate map: "
101081cc8:	e0 43 01 91	add	x0, sp, #0x50
101081ccc:	82 04 80 52	mov	w2, #0x24
101081cd0:	b8 53 be 97	bl	0x100016bb0
101081cd4:	81 81 8f 94	bl	0x1034622d8 ; symbol stub for: __ZNSt3__16chrono12steady_clock3nowEv
101081cd8:	08 00 14 cb	sub	x8, x0, x20
101081cdc:	e9 f9 9e d2	mov	x9, #0xf7cf
101081ce0:	69 6a bc f2	movk	x9, #0xe353, lsl #16
101081ce4:	a9 74 d3 f2	movk	x9, #0x9ba5, lsl #32
101081ce8:	89 18 e4 f2	movk	x9, #0x20c4, lsl #48
101081cec:	08 7d 49 9b	smulh	x8, x8, x9
101081cf0:	09 fd 47 93	asr	x9, x8, #7
101081cf4:	28 fd 48 8b	add	x8, x9, x8, lsr #63
101081cf8:	00 01 62 9e	scvtf	d0, x8
101081cfc:	08 00 c8 d2	mov	x8, #0x400000000000
101081d00:	e8 11 e8 f2	movk	x8, #0x408f, lsl #48
101081d04:	01 01 67 9e	fmov	d1, x8
101081d08:	00 18 61 1e	fdiv	d0, d0, d1
101081d0c:	e0 43 01 91	add	x0, sp, #0x50
101081d10:	af 80 8f 94	bl	0x103461fcc ; symbol stub for: __ZNSt3__113basic_ostreamIcNS_11char_traitsIcEEElsEd
101081d14:	e0 43 01 91	add	x0, sp, #0x50
101081d18:	ae a8 be 97	bl	__ZN3ins11LLogMessageD1Ev
101081d1c:	e8 df c0 39	ldrsb	w8, [sp, #0x37]
101081d20:	28 06 f8 37	tbnz	w8, #0x1f, 0x101081de4
101081d24:	e8 3f c1 39	ldrsb	w8, [sp, #0x4f]
101081d28:	68 06 f8 37	tbnz	w8, #0x1f, 0x101081df4
101081d2c:	68 82 41 39	ldrb	w8, [x19, #0x60]
101081d30:	a8 06 00 34	cbz	w8, 0x101081e04
101081d34:	e1 03 40 ad	ldp	q1, q0, [sp]
101081d38:	60 06 02 ad	stp	q0, q1, [x19, #0x40]
101081d3c:	00 68 61 0e	fcvtn	v0.2s, v0.2d
101081d40:	20 68 61 4e	fcvtn2	v0.4s, v1.2d
101081d44:	e0 17 80 3d	str	q0, [sp, #0x50]
101081d48:	e0 43 01 91	add	x0, sp, #0x50
101081d4c:	c4 d6 1f 94	bl	__ZN9arvrender10Stabilizer19QuatFromGyrostablibERKN3glm5tquatIfLNS1_9precisionE0EEE
101081d50:	04 1c a0 4e	mov.16b	v4, v0
101081d54:	24 04 0c 6e	mov.s	v4[1], v1[0]
101081d58:	45 40 21 1e	fneg	s5, s2
101081d5c:	00 08 20 1e	fmul	s0, s0, s0
101081d60:	21 08 21 1e	fmul	s1, s1, s1
101081d64:	42 08 22 1e	fmul	s2, s2, s2
101081d68:	66 08 23 1e	fmul	s6, s3, s3
101081d6c:	00 28 21 1e	fadd	s0, s0, s1
101081d70:	41 28 26 1e	fadd	s1, s2, s6
101081d74:	00 28 21 1e	fadd	s0, s0, s1
101081d78:	68 02 42 39	ldrb	w8, [x19, #0x80]
101081d7c:	81 f8 a0 6e	fneg.4s	v1, v4
101081d80:	a1 04 14 6e	mov.s	v1[2], v5[0]
101081d84:	61 04 1c 6e	mov.s	v1[3], v3[0]
101081d88:	00 04 04 4e	dup.4s	v0, v0[0]
101081d8c:	20 fc 20 6e	fdiv.4s	v0, v1, v0
101081d90:	60 1e 80 3d	str	q0, [x19, #0x70]
101081d94:	68 00 00 35	cbnz	w8, 0x101081da0
101081d98:	28 00 80 52	mov	w8, #0x1
101081d9c:	68 02 02 39	strb	w8, [x19, #0x80]
101081da0:	a0 03 51 f8	ldur	x0, [x29, #-0xf0]
101081da4:	60 00 00 b4	cbz	x0, 0x101081db0
101081da8:	a0 83 11 f8	stur	x0, [x29, #-0xe8]
101081dac:	db 81 8f 94	bl	0x103462518 ; symbol stub for: __ZdlPv
101081db0:	a0 83 52 f8	ldur	x0, [x29, #-0xd8]
101081db4:	60 00 00 b4	cbz	x0, 0x101081dc0
101081db8:	a0 03 13 f8	stur	x0, [x29, #-0xd0]
101081dbc:	d7 81 8f 94	bl	0x103462518 ; symbol stub for: __ZdlPv
101081dc0:	ff 83 0a 91	add	sp, sp, #0x2a0
101081dc4:	fd 7b 46 a9	ldp	x29, x30, [sp, #0x60]
101081dc8:	f4 4f 45 a9	ldp	x20, x19, [sp, #0x50]
101081dcc:	f6 57 44 a9	ldp	x22, x21, [sp, #0x40]
101081dd0:	f8 5f 43 a9	ldp	x24, x23, [sp, #0x30]
101081dd4:	fa 67 42 a9	ldp	x26, x25, [sp, #0x20]
101081dd8:	fc 6f 41 a9	ldp	x28, x27, [sp, #0x10]
101081ddc:	e9 23 c7 6c	ldp	d9, d8, [sp], #0x70
101081de0:	c0 03 5f d6	ret
101081de4:	e0 13 40 f9	ldr	x0, [sp, #0x20]
101081de8:	cc 81 8f 94	bl	0x103462518 ; symbol stub for: __ZdlPv
101081dec:	e8 3f c1 39	ldrsb	w8, [sp, #0x4f]
101081df0:	e8 f9 ff 36	tbz	w8, #0x1f, 0x101081d2c
101081df4:	e0 1f 40 f9	ldr	x0, [sp, #0x38]
101081df8:	c8 81 8f 94	bl	0x103462518 ; symbol stub for: __ZdlPv
101081dfc:	68 82 41 39	ldrb	w8, [x19, #0x60]
101081e00:	a8 f9 ff 35	cbnz	w8, 0x101081d34
101081e04:	28 00 80 52	mov	w8, #0x1
101081e08:	68 82 01 39	strb	w8, [x19, #0x60]
101081e0c:	ca ff ff 17	b	0x101081d34
101081e10:	e0 e3 00 91	add	x0, sp, #0x38
101081e14:	29 54 be 97	bl	0x100016eb8
101081e18:	11 00 00 14	b	0x101081e5c
101081e1c:	a0 c3 03 d1	sub	x0, x29, #0xf0
101081e20:	1f 88 be 97	bl	0x100023e9c
101081e24:	0e 00 00 14	b	0x101081e5c
101081e28:	e0 e3 05 91	add	x0, sp, #0x178
101081e2c:	1c 88 be 97	bl	0x100023e9c
101081e30:	0b 00 00 14	b	0x101081e5c
101081e34:	e0 83 05 91	add	x0, sp, #0x160
101081e38:	19 88 be 97	bl	0x100023e9c
101081e3c:	08 00 00 14	b	0x101081e5c
101081e40:	a0 63 03 d1	sub	x0, x29, #0xd8
101081e44:	16 88 be 97	bl	0x100023e9c
101081e48:	05 00 00 14	b	0x101081e5c
101081e4c:	a0 c3 03 d1	sub	x0, x29, #0xf0
101081e50:	13 88 be 97	bl	0x100023e9c
101081e54:	02 00 00 14	b	0x101081e5c
101081e58:	2c af be 97	bl	0x10002db08
101081e5c:	20 00 20 d4	brk	#0x1
101081e60:	25 00 00 14	b	0x101081ef4
101081e64:	24 00 00 14	b	0x101081ef4
101081e68:	03 00 00 14	b	0x101081e74
101081e6c:	22 00 00 14	b	0x101081ef4
101081e70:	21 00 00 14	b	0x101081ef4
101081e74:	f3 03 00 aa	mov	x19, x0
101081e78:	e0 b3 40 f9	ldr	x0, [sp, #0x160]
101081e7c:	a0 00 00 b4	cbz	x0, 0x101081e90
101081e80:	e0 b7 00 f9	str	x0, [sp, #0x168]
101081e84:	a5 81 8f 94	bl	0x103462518 ; symbol stub for: __ZdlPv
101081e88:	02 00 00 14	b	0x101081e90
101081e8c:	f3 03 00 aa	mov	x19, x0
101081e90:	e0 bf 40 f9	ldr	x0, [sp, #0x178]
101081e94:	20 03 00 b4	cbz	x0, 0x101081ef8
101081e98:	e0 c3 00 f9	str	x0, [sp, #0x180]
101081e9c:	9f 81 8f 94	bl	0x103462518 ; symbol stub for: __ZdlPv
101081ea0:	16 00 00 14	b	0x101081ef8
101081ea4:	14 00 00 14	b	0x101081ef4
101081ea8:	13 00 00 14	b	0x101081ef4
101081eac:	01 00 00 14	b	0x101081eb0
101081eb0:	f3 03 00 aa	mov	x19, x0
101081eb4:	e0 43 01 91	add	x0, sp, #0x50
101081eb8:	46 a8 be 97	bl	__ZN3ins11LLogMessageD1Ev
101081ebc:	02 00 00 14	b	0x101081ec4
101081ec0:	f3 03 00 aa	mov	x19, x0
101081ec4:	e8 df c0 39	ldrsb	w8, [sp, #0x37]
101081ec8:	68 00 f8 36	tbz	w8, #0x1f, 0x101081ed4
101081ecc:	e0 13 40 f9	ldr	x0, [sp, #0x20]
101081ed0:	92 81 8f 94	bl	0x103462518 ; symbol stub for: __ZdlPv
101081ed4:	e8 3f c1 39	ldrsb	w8, [sp, #0x4f]
101081ed8:	08 01 f8 36	tbz	w8, #0x1f, 0x101081ef8
101081edc:	e0 1f 40 f9	ldr	x0, [sp, #0x38]
101081ee0:	8e 81 8f 94	bl	0x103462518 ; symbol stub for: __ZdlPv
101081ee4:	05 00 00 14	b	0x101081ef8
101081ee8:	03 00 00 14	b	0x101081ef4
101081eec:	02 00 00 14	b	0x101081ef4
101081ef0:	01 00 00 14	b	0x101081ef4
101081ef4:	f3 03 00 aa	mov	x19, x0
101081ef8:	a0 03 51 f8	ldur	x0, [x29, #-0xf0]
101081efc:	a0 00 00 b5	cbnz	x0, 0x101081f10
101081f00:	a0 83 52 f8	ldur	x0, [x29, #-0xd8]
101081f04:	e0 00 00 b5	cbnz	x0, 0x101081f20
101081f08:	e0 03 13 aa	mov	x0, x19
101081f0c:	5c 66 8f 94	bl	0x10345b87c ; symbol stub for: __Unwind_Resume
101081f10:	a0 83 11 f8	stur	x0, [x29, #-0xe8]
101081f14:	81 81 8f 94	bl	0x103462518 ; symbol stub for: __ZdlPv
101081f18:	a0 83 52 f8	ldur	x0, [x29, #-0xd8]
101081f1c:	60 ff ff b4	cbz	x0, 0x101081f08
101081f20:	a0 03 13 f8	stur	x0, [x29, #-0xd0]
101081f24:	7d 81 8f 94	bl	0x103462518 ; symbol stub for: __ZdlPv
101081f28:	e0 03 13 aa	mov	x0, x19
101081f2c:	54 66 8f 94	bl	0x10345b87c ; symbol stub for: __Unwind_Resume
