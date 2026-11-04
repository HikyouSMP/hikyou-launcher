import { mount } from 'svelte';
import '../../src/index.css';
import Fixture from './modSelections.fixture.svelte';
import i18n from '../../src/i18n';
await i18n.changeLanguage('en');
const target=document.getElementById('root');
if(!target)throw new Error('Missing fixture root');
mount(Fixture,{target});
