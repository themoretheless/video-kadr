import { mount } from 'svelte'
import './../../src/app.css'
import Player from './LiveDspEditor.svelte'
mount(Player, { target: document.querySelector('#app')! })
