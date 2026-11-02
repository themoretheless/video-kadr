import {test,expect} from '@playwright/test'
if(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE)test.use({launchOptions:{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE}})
test('paints native parade pixels, labels and foreground at wide and narrow widths',async({page})=>{
 await page.goto('/e2e/fixtures/native-scope.html')
 const canvas=page.locator('canvas');await expect(canvas).toBeVisible()
 await expect(page.locator('svg.foreground line')).toHaveCount(2)
 expect(await page.locator('svg line').first().evaluate(element=>getComputedStyle(element).vectorEffect)).toBe('non-scaling-stroke')
 await expect(page.locator('.axis.left span')).toHaveCount(11)
 await expect(page.locator('.axis.right span').nth(5)).toHaveText('128')
 expect(await canvas.evaluate(element=>{const node=element as HTMLCanvasElement;return Array.from(node.getContext('2d')!.getImageData(0,0,node.width,node.height).data).some(v=>v>0)})).toBe(true)
 for(const width of [1000,360]){
  await page.setViewportSize({width,height:600})
  const image=await canvas.boundingBox();const overlay=await page.locator('svg.foreground').boundingBox()
  expect(image).not.toBeNull();expect(overlay).not.toBeNull()
  expect(Math.abs(image!.width-overlay!.width)).toBeLessThan(1);expect(Math.abs(image!.height-overlay!.height)).toBeLessThan(1)
  expect(await page.locator('svg.foreground').evaluate(element=>Number(getComputedStyle(element).zIndex))).toBeGreaterThan(await canvas.evaluate(element=>Number(getComputedStyle(element).zIndex)))
 }
 await page.screenshot({path:process.env.SCOPE_SCREENSHOT_PATH??'test-results/native-parade.png'})
})
