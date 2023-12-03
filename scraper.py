import json
from datetime import datetime

from sqlalchemy import text
from sqlalchemy.exc import IntegrityError
from selenium.common.exceptions import TimeoutException, StaleElementReferenceException
from selenium.webdriver import Chrome
from selenium.webdriver.chrome.options import Options
from selenium.webdriver.support.ui import WebDriverWait
from selenium.webdriver.common.by import By

from db import db_ctx, Product, ProductScrapeStatus

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


class wait_for_product_jsonld:
    def __call__(self, driver):
        try:
            jsonld_element = driver.find_element(
                By.CSS_SELECTOR,
                'script[type="application/ld+json"]:last-of-type'
            )
            self.jsonld = jsonld_element.get_property('innerHTML')
            return '"@type":"Product"' in self.jsonld
        except StaleElementReferenceException:
            return False


def get_seller_from_url(url: str) -> str:
    if url.startswith('https://groceries.asda.com'):
        return 'asda'
    elif url.startswith('https://www.sainsburys.co.uk'):
        return 'sainsburys'
    else:
        raise ValueError(f'Cannot identify seller for url {url}')


def scrape_asda_product(url: str):
    chrome_options = Options()
    chrome_options.add_argument("--headless")

    browser = Chrome(options=chrome_options)
    browser.get(url)
    condition = wait_for_product_jsonld()
    try:
        WebDriverWait(browser, 30).until(condition)
    except TimeoutException:
        return 'FAILURE_TIMEOUT'
    json_ld = json.loads(condition.jsonld)
    # soup = BeautifulSoup(browser.page_source, features="html.parser")
    browser.quit()

    # code = soup.find('span', class_='pdp-main-details__product-code').text.strip('Product code: ')
    # title = soup.find('h1', class_='pdp-main-details__title').text
    # price_per = soup.find('span', class_="co-product__price-per-uom").text
    # price = list(soup.find('strong', class_='pdp-main-details__price').strings)[-1]
    # nutritional_values = soup.find('div', class_='pdp-description-reviews__product-details-title', string='Nutritional Values').parent
    # net_content = soup.find('div', class_='pdp-description-reviews__product-details-title', string='Net Content').parent.find('div', class_='pdp-description-reviews__product-details-content').text

    # json_ld = json.loads(soup.find_all('script', type="application/ld+json")[-1].text)
    if 'name' not in json_ld:
        return 'FAILURE_MISSING_NAME'

    with db_ctx() as db:
        product = Product(
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
            seller = get_seller_from_url(url),
            scraped = datetime.now()
        )
        try:
            db.add(product)
            db.commit()
        except IntegrityError:
            return 'FAILURE_DUPLICATED_URL'
    
    return 'SUCCESS'

if __name__ == '__main__':
    with db_ctx() as db:
        query = """
        SELECT url FROM productscrapestatus
        WHERE
            last_scraped < NOW() - interval '2 day'
            OR last_scraped IS NULL
        ORDER BY last_scraped NULLS FIRST
        """
        urls_to_scrape = db.execute(text(query)).scalars().all()

    for url in urls_to_scrape:
        print(url, end='', flush=True)
        status = scrape_asda_product(url)

        with db_ctx() as db:
            (
                db.query(ProductScrapeStatus)
                .filter(ProductScrapeStatus.url == url)
                .update(dict(
                    last_scraped = datetime.now(),
                    scrape_success = status == 'SUCCESS',
                    fail_reason = None if status == 'SUCCESS' else status
                ))
            )
            db.commit()

        
        print(f' - {bcolors.GREEN if status == "SUCCESS" else bcolors.RED}{status}{bcolors.ENDC}')