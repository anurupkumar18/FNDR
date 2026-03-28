const puppeteer = require('puppeteer');

(async () => {
  const browser = await puppeteer.launch();
  const page = await browser.newPage();
  
  page.on('console', msg => {
    console.log(`[Browser Console]: ${msg.text()}`);
  });
  
  page.on('pageerror', error => {
    console.log(`[Browser Error]: ${error.message}`);
  });
  
  await page.goto('http://localhost:1420', { waitUntil: 'networkidle0' });
  await browser.close();
})();
