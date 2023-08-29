import json
from datetime import datetime

from sqlalchemy import text
from bs4 import BeautifulSoup
from selenium.common.exceptions import TimeoutException
from selenium.webdriver import Chrome
from selenium.webdriver.chrome.options import Options
from selenium.webdriver.support.ui import WebDriverWait
from selenium.webdriver.support import expected_conditions
from selenium.webdriver.common.by import By

from db import db_ctx, Product

class bcolors:
    HEADER = '\033[95m'
    OKBLUE = '\033[94m'
    OKCYAN = '\033[96m'
    OKGREEN = '\033[92m'
    GREEN = '\033[32m'
    RED = '\033[31m'
    WARNING = '\033[93m'
    FAIL = '\033[91m'
    ENDC = '\033[0m'
    BOLD = '\033[1m'
    UNDERLINE = '\033[4m'


def update_last_scraped_val(url: str):
    with db_ctx() as db:
        (
            db.query(Product)
            .filter(Product.url == url)
            .update(dict(last_scraped = datetime.now()))
        )
        db.commit()


def scrape_asda_product(url: str):
    chrome_options = Options()
    chrome_options.add_argument("--headless")

    browser = Chrome(options=chrome_options)
    browser.get(url)
    condition = expected_conditions.presence_of_element_located((By.CLASS_NAME, "pdp-description-reviews__product-details-title"))
    try:
        WebDriverWait(browser, 30).until(condition)
    except TimeoutException:
        update_last_scraped_val(url)
        return 'FAILURE_TIMEOUT'
    soup = BeautifulSoup(browser.page_source, features="html.parser")
    browser.quit()

    # code = soup.find('span', class_='pdp-main-details__product-code').text.strip('Product code: ')
    # title = soup.find('h1', class_='pdp-main-details__title').text
    # price_per = soup.find('span', class_="co-product__price-per-uom").text
    # price = list(soup.find('strong', class_='pdp-main-details__price').strings)[-1]
    # nutritional_values = soup.find('div', class_='pdp-description-reviews__product-details-title', string='Nutritional Values').parent
    # net_content = soup.find('div', class_='pdp-description-reviews__product-details-title', string='Net Content').parent.find('div', class_='pdp-description-reviews__product-details-content').text

    json_ld = json.loads(soup.find_all('script', type="application/ld+json")[-1].text)

    if 'name' not in json_ld:
        update_last_scraped_val(url)
        return 'FAILURE_MISSING_NAME'

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
            last_scraped = datetime.now()
        )
        (
            db.query(Product)
            .filter(Product.url == url)
            .update(update_params)
        )
        db.commit()
    
    return 'SUCCESS'

with db_ctx() as db:
    urls_to_scrape = db.execute(text('SELECT url FROM product WHERE last_scraped IS NULL')).scalars().all()

for url in urls_to_scrape:
    print(url, end='', flush=True)
    status = scrape_asda_product(url)
    
    print(f' - {bcolors.GREEN if status == "SUCCESS" else bcolors.RED}{status}{bcolors.ENDC}')