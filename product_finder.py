import json
import re
import time

from sqlalchemy.exc import IntegrityError

from bs4 import BeautifulSoup
from selenium.webdriver import Chrome
from selenium.webdriver.chrome.options import Options

from db import db_ctx, Product

url = 'https://groceries.asda.com/'
chrome_options = Options()
# chrome_options.add_argument("--headless")

found_products = set()
found_cats = set()
browser = Chrome(options=chrome_options)
browser.get(url)
while True:
    try:
        soup = BeautifulSoup(browser.page_source, features="html.parser")
        found_products |= {'https://groceries.asda.com/product/' + x.group(1) 
            for x in re.finditer(r'/product/([\-/a-zA-Z0-9]+)', str(soup))
        }
        found_cats |= {'https://groceries.asda.com/cat/' + x.group(1) 
            for x in re.finditer(r'/cat/([\-/a-zA-Z0-9]+)', str(soup))
        }
        print(f'{len(found_cats)} {len(found_products)}', flush=True, end='\r')
        time.sleep(0.5)
    except KeyboardInterrupt:
        break
browser.quit()
