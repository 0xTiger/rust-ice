import json

from sqlalchemy import text

from bs4 import BeautifulSoup
from selenium.common.exceptions import TimeoutException
from selenium.webdriver import Chrome
from selenium.webdriver.chrome.options import Options
from selenium.webdriver.support.ui import WebDriverWait
from selenium.webdriver.support import expected_conditions
from selenium.webdriver.common.by import By

from db import db_ctx, Product


def scrape_asda_product(url: str):
    chrome_options = Options()
    chrome_options.add_argument("--headless")

    browser = Chrome(options=chrome_options)
    browser.get(url)
    condition = expected_conditions.presence_of_element_located((By.CLASS_NAME, "pdp-description-reviews__product-details-title"))
    try:
        WebDriverWait(browser, 30).until(condition)
    except TimeoutException:
        return
    soup = BeautifulSoup(browser.page_source, features="html.parser")
    browser.quit()

    # code = soup.find('span', class_='pdp-main-details__product-code').text.strip('Product code: ')
    # title = soup.find('h1', class_='pdp-main-details__title').text
    # price_per = soup.find('span', class_="co-product__price-per-uom").text
    # price = list(soup.find('strong', class_='pdp-main-details__price').strings)[-1]
    # nutritional_values = soup.find('div', class_='pdp-description-reviews__product-details-title', string='Nutritional Values').parent
    # net_content = soup.find('div', class_='pdp-description-reviews__product-details-title', string='Net Content').parent.find('div', class_='pdp-description-reviews__product-details-content').text

    json_ld = json.loads(soup.find_all('script', type="application/ld+json")[-1].text)
    # found_urls = {'https://groceries.asda.com/product/' + x.group(1) 
    #     for x in re.finditer(r'/product/([\-/a-zA-Z0-9]+)', str(soup))
    # } - {url}
    # if len(found_urls) > 0:
    #     print('Found urls:')
    #     for url in found_urls:
    #         print(f'-   {url}')

    if 'name' not in json_ld: return
    print(f'Scraped {json_ld["name"]}')

    with db_ctx() as db:
        update_params = dict(
            gtin = json_ld['gtin'],
            json_ld = json_ld,
            name = json_ld['name'],
            sku = json_ld['sku'],
            image = json_ld['image'],
            description = json_ld['description'],
            rating = json_ld.get('aggregateRating', dict()).get('ratingValue'),
            review_count = json_ld.get('aggregateRating', dict()).get('reviewCount', 0),
            brand = json_ld['brand']['name'],
            price = json_ld['offers']['price'],
            url = json_ld['offers']['url'],
            availability = json_ld['offers']['availability'],
        )
        (
            db.query(Product)
            .filter(Product.url == url)
            .update(update_params)
        )
        db.commit()

with db_ctx() as db:
    urls_to_scrape = db.execute(text('SELECT url FROM product WHERE gtin IS NULL')).scalars().all()

for url in urls_to_scrape:
    print(url)
    scrape_asda_product(url)