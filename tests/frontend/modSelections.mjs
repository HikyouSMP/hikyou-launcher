import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdir,readFile,writeFile } from 'node:fs/promises';
import { join,resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createServer } from 'vite';
const en=JSON.parse(await readFile('src/locales/en.json','utf8'));
const ja=JSON.parse(await readFile('src/locales/ja.json','utf8'));
const require=createRequire(process.env.HIKYOU_TEST_RUNTIME_MODULES?join(resolve(process.env.HIKYOU_TEST_RUNTIME_MODULES),'__selections__.cjs'):import.meta.url);
const imported=await import(pathToFileURL(require.resolve('playwright')).href);
const {chromium}=imported.chromium?imported:imported.default;
const output=resolve('.local/mod-selections');await mkdir(output,{recursive:true});
const server=await createServer({server:{host:'127.0.0.1',port:0}});
let browser,page;const errors=[],external=[];let passed=false;
try{
  await server.listen();const origin=`http://127.0.0.1:${server.httpServer.address().port}`;
  browser=await chromium.launch({headless:true,channel:process.env.HIKYOU_TEST_BROWSER_CHANNEL||undefined});
  const context=await browser.newContext({viewport:{width:750,height:600},serviceWorkers:'block'});
  await context.route('**/*',async route=>{
    const url=new URL(route.request().url());
    if(url.origin!==origin){external.push(url.href);return route.abort();}
    if(url.pathname==='/src/main.ts')return route.fulfill({contentType:'text/javascript',body:"import '/tests/frontend/modSelections.entry.ts';"});
    return route.continue();
  });
  page=await context.newPage();page.setDefaultTimeout(30000);page.on('pageerror',e=>errors.push(e.message));
  await page.goto(origin,{waitUntil:'domcontentloaded',timeout:90000});
  const row=page.locator('.template-row').filter({hasText:'Sodium'});
  await row.waitFor();await row.click();
  assert.equal(await page.locator('.template-row').count(),2);
  const scope=page.getByLabel(en.mod_template.apply_on,{exact:true});
  await page.getByRole('button',{name:'Fail next save',exact:true}).click();
  await scope.selectOption('fabric');
  await page.getByRole('alert').waitFor();
  await scope.waitFor({state:'visible'});
  await page.waitForFunction(()=>!document.querySelector('.template-details select')?.disabled);
  assert.equal(await scope.inputValue(),'','failed writes do not become authoritative');
  await scope.selectOption('fabric');await page.waitForFunction(()=>document.querySelector('.template-details select')?.value==='fabric');
  await page.getByLabel(en.mod_template.folder_name,{exact:true}).fill('Performance');
  await page.getByTitle(en.mod_template.new_folder,{exact:true}).click();
  await page.locator('.template-details select').nth(1).selectOption({label:'Performance'});
  await page.locator('.template-toolbar select').first().selectOption('neoforge');
  assert.equal(await page.locator('.template-row').count(),0,'filter changes visibility only');
  await page.locator('.template-toolbar select').first().selectOption('');
  assert.equal(await page.locator('.template-row').count(),2);
  await page.screenshot({path:join(output,'preinstallation-en.png')});
  await page.getByRole('button',{name:'Language',exact:true}).click();
  await page.getByRole('heading',{name:ja.mod_template.title,exact:true}).waitFor();
  await page.setViewportSize({width:520,height:700});
  assert.equal(await page.locator('.template-editor').evaluate(el=>el.scrollWidth<=el.clientWidth),true);
  await page.screenshot({path:join(output,'preinstallation-ja-narrow.png')});
  await page.getByRole('button',{name:'Language',exact:true}).click();
  await page.getByRole('button',{name:'Plus setup',exact:true}).click();
  await page.getByRole('button',{name:en.plus_setup.review,exact:true}).click();
  const unsupported=page.locator('[data-plus-setup] .py-2').filter({hasText:'Other Loader Mod'});
  assert.equal(await unsupported.getByRole('button').isDisabled(),true);
  const sodium=page.locator('[data-plus-setup] .py-2').filter({hasText:'Sodium'});
  assert.equal(await sodium.getByRole('button').getAttribute('aria-pressed'),'false');
  await sodium.getByRole('button').click();
  await page.screenshot({path:join(output,'plus-play-review.png')});
  await page.getByRole('button',{name:en.plus_setup.confirm,exact:true}).click();
  await page.waitForFunction(()=>document.querySelector('[data-confirmed]')?.getAttribute('data-confirmed')==='1');
  await page.getByRole('button',{name:'Create profile',exact:true}).click();
  const dialog=page.getByRole('dialog');await dialog.waitFor();
  assert.ok((await dialog.innerText()).includes('Later Mod'));
  assert.ok((await dialog.innerText()).includes(en.mod_template.unavailable));
  await page.screenshot({path:join(output,'creation-review.png')});
  await dialog.getByRole('button',{name:en.plus_setup.selected,exact:true}).click();
  await dialog.getByRole('button',{name:en.mod_template.use_available,exact:true}).click();
  assert.equal(await page.locator('[data-answer]').getAttribute('data-answer'),'[]');
  assert.deepEqual(errors,[]);assert.deepEqual(external,[]);passed=true;
}catch(error){
  if(page){await page.screenshot({path:join(output,'failure.png')});console.log((await page.locator('body').innerText()).slice(0,2500));}
  throw error;
}finally{
  await browser?.close();await server.close();
  await writeFile(join(output,'summary.json'),JSON.stringify({passed,errors,external},null,2));
}
console.log('Mod selection synthetic UI: passed');
