import {expect,test} from '@playwright/test'
if(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE)test.use({launchOptions:{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE}})
test('whole-project fonts include unused assets once and replace with atomic locks and Undo/Redo',async({page,request})=>{
 const registration=await request.post('/api/auth/register',{data:{username:'project-fonts-browser',password:'project-fonts-password'}})
 expect(registration.status()).toBe(201)
 const {token}=await registration.json();const headers={Authorization:`Bearer ${token}`}
 const catalog=await request.get('/api/compositions/graphic-templates',{headers});expect(catalog.status()).toBe(200)
 const template=(await catalog.json())[0]
 const constant=(value:number)=>({mode:'constant',value})
 const transform={x:constant(0),y:constant(0),scaleX:constant(1),scaleY:constant(1),rotationDegrees:constant(0),anchorX:.5,anchorY:.5}
 const empty={schemaVersion:1,timeBase:1000000,canvas:{width:80,height:64,fpsMilli:24000,background:{red:0,green:0,blue:0,alpha:1}},sources:{},tracks:[]}
 const content={...empty,tracks:[{kind:'graphic',id:'graphics',name:'Graphics',hidden:true,locked:false,clips:[{id:'title',timelineStartTick:0,timelineEndTick:1000000,width:80,height:64,graphic:template.graphic,effects:[],transform,opacity:constant(1),enabled:false}]}]}
 const instances=['first','second'].map(id=>({id,timing:{composition:'child',timelineStartUs:0,sourceInUs:0,sourceOutUs:1000000},transform,opacity:constant(1),blendMode:'normal'}))
 const project={schemaVersion:1,root:'root',compositions:{root:{durationUs:1000000,content:empty,instances},child:{durationUs:1000000,content,instances:[]},unused:{durationUs:1000000,content:structuredClone(content),instances:[]}}}
 const single=await request.post('/api/compositions/fonts-used',{headers,data:content});expect(single.status()).toBe(200)
 const expected=(await single.json()).map((font:{count:number})=>({...font,count:font.count*2}))
 const originalFace=expected.find((font:{family:string;style:string})=>font.family==='Inter'&&font.style!=='Regular');expect(originalFace).toBeTruthy()
 const missingContent=structuredClone(content)
 const missingLayer=missingContent.tracks[0].clips[0].graphic.layers.find((layer:{content:{kind:string}})=>layer.content.kind==='text')
 missingLayer.content.parameters.font='Missing-Browser-Font-1502'
 const missingStatus=await request.post('/api/compositions/fonts-used',{headers,data:missingContent});expect(missingStatus.status()).toBe(200)
 expect(await missingStatus.json()).toEqual(expect.arrayContaining([expect.objectContaining({family:'Missing-Browser-Font-1502',missing:true}),expect.objectContaining({family:'Inter',missing:false})]))
 const used=await request.post('/api/compositions/project-fonts-used',{headers,data:project});expect(used.status()).toBe(200);expect(await used.json()).toEqual(expected)
 const locked=structuredClone(project);locked.compositions.unused.content.tracks[0].locked=true
 const refused=await request.post('/api/compositions/project-replace-fonts',{headers,data:{project:locked,from:'Inter',to:'External'}});expect(refused.status()).toBe(400)
 await page.addInitScript(value=>localStorage.setItem('video-kadr.live-project-fonts-test',JSON.stringify(value)),{token,project})
 const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message))
 await page.goto('/e2e/fixtures/live-project-fonts.html')
 await page.getByText('Шрифты всего проекта',{exact:true}).click()
 await expect(page.locator('li').filter({hasText:`Inter · ${originalFace.style}`} )).toBeVisible()
 await page.getByLabel('Исходный шрифт проекта',{exact:true}).selectOption(`Inter/${originalFace.style}`)
 await page.getByLabel('Новое семейство шрифта проекта',{exact:true}).fill('Inter')
 await page.getByLabel('Новое начертание шрифта проекта',{exact:true}).fill('Regular')
 const replacement=page.waitForResponse(response=>response.url().endsWith('/api/compositions/project-replace-fonts')&&response.request().method()==='POST')
 await page.getByRole('button',{name:'Заменить шрифт во всём проекте',exact:true}).click()
 expect((await replacement).status()).toBe(200)
 await expect(page.getByLabel('Project font history')).toHaveText('1')
 await expect(page.getByLabel('Project renders')).toHaveText('{}')
 const changed=JSON.parse(await page.getByLabel('Project document').innerText())
 expect(changed.compositions.root.instances).toEqual(project.compositions.root.instances)
 for(const id of ['child','unused'])expect(changed.compositions[id].content.tracks[0].clips[0].graphic.layers.filter((layer:{content:{kind:string;parameters:{fontStyle:string}}})=>layer.content.kind==='text').some((layer:{content:{parameters:{fontStyle:string}}})=>layer.content.parameters.fontStyle===originalFace.style)).toBe(false)
 await page.getByRole('button',{name:'Undo',exact:true}).click()
 await expect(page.getByLabel('Project renders')).toHaveText('{"root":"previous-render"}')
 expect(JSON.parse(await page.getByLabel('Project document').innerText())).toEqual(project)
 await page.getByRole('button',{name:'Redo',exact:true}).click()
 await expect(page.getByLabel('Project renders')).toHaveText('{}')
 expect(JSON.parse(await page.getByLabel('Project document').innerText())).toEqual(changed)
 const missingProjectResponse=page.waitForResponse(response=>response.url().endsWith('/api/compositions/project-fonts-used')&&response.request().method()==='POST')
 await page.getByRole('button',{name:'Проверить отсутствующий шрифт',exact:true}).click()
 expect((await missingProjectResponse).status()).toBe(200)
 await expect(page.locator('li').filter({hasText:'Missing-Browser-Font-1502'}).getByText('Не найден',{exact:true})).toBeVisible()
 expect(errors).toEqual([])
 await page.locator('details').screenshot({path:'/tmp/video-kadr-stage1501-project-fonts.png'})
})
