import json

from bs4 import BeautifulSoup
from selenium.webdriver import Chrome
from selenium.webdriver.chrome.options import Options
from selenium.webdriver.support.ui import WebDriverWait
from selenium.webdriver.support import expected_conditions as EC
from selenium.webdriver.common.by import By

from db import db_ctx, Product

chrome_options = Options()
chrome_options.add_argument("--headless")

browser = Chrome(options=chrome_options)

url = 'https://groceries.asda.com/product/beef-steaks-sauces/asda-succulent-prime-beef-sirloin-steak/910000993334'
# url = 'https://groceries.asda.com/product/beef-steaks-sauces/asda-creamy-peppercorn-sauce/1000271205950'

def scrape_asda_product(url: str):
    browser.get(url)
    condition = EC.presence_of_element_located((By.CLASS_NAME, "pdp-description-reviews__product-details-title"))
    WebDriverWait(browser, 3).until(condition)
    soup = BeautifulSoup(browser.page_source)
    browser.quit()

    code = soup.find('span', class_='pdp-main-details__product-code').text.strip('Product code: ')
    title = soup.find('h1', class_='pdp-main-details__title').text
    price_per = soup.find('span', class_="co-product__price-per-uom").text
    price = list(soup.find('strong', class_='pdp-main-details__price').strings)[-1]
    nutritional_values = soup.find('div', class_='pdp-description-reviews__product-details-title', string='Nutritional Values').parent
    net_content = soup.find('div', class_='pdp-description-reviews__product-details-title', string='Net Content').parent.find('div', class_='pdp-description-reviews__product-details-content').text

    json_ld = json.loads(soup.find_all('script', type="application/ld+json")[-1].text)
    print('-----------')
    print(f'{code=}')
    print(f'{title=}')
    print(f'{price_per=}')
    print(f'{price=}')
    print(f'{net_content=}')
    print(f'{json_ld=}')

    with db_ctx() as db:
        product = Product(
            gtin = json_ld['gtin'],
            json_ld = json_ld,
            name = json_ld['name'],
            sku = json_ld['sku'],
            image = json_ld['image'],
            description = json_ld['description'],
            rating = json_ld['aggregateRating']['ratingValue'],
            review_count = json_ld['aggregateRating']['reviewCount'],
            brand = json_ld['brand']['name'],

            price = json_ld['offers']['price'],
            url = json_ld['offers']['url'],
            availability = json_ld['offers']['availability'],
        )
        db.add(product)
        db.commit()

