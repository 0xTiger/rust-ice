import re
import time

from sqlalchemy import text
from bs4 import BeautifulSoup
from selenium.webdriver import Chrome
from selenium.common.exceptions import NoSuchWindowException

from db import db_ctx
from scraper import bcolors


url = 'https://groceries.asda.com/'
browser = Chrome()
browser.get(url)


with db_ctx() as db:
    existing_products = set(db.execute(text('SELECT url FROM product')).scalars())
found_products = set()
found_cats = set()
while True:
    try:
        soup = BeautifulSoup(browser.page_source, features="html.parser")
        found_products |= {'https://groceries.asda.com/product/' + x.group(1) 
            for x in re.finditer(r'/product/([\-/a-zA-Z0-9]+)', str(soup))
        }
        found_cats |= {'https://groceries.asda.com/cat/' + x.group(1) 
            for x in re.finditer(r'/cat/([\-/a-zA-Z0-9]+)', str(soup))
        }
        infostr = f'{len(found_cats)} {len(found_products)} {bcolors.GREEN}+({len(found_products - existing_products)}){bcolors.ENDC}'
        print(infostr, flush=True, end='\r')
        time.sleep(0.1)
    except (KeyboardInterrupt, NoSuchWindowException):
        break
browser.quit()


with db_ctx() as db:
    for url in found_products:
        db.execute(
            text('INSERT INTO productscrapestatus (url) VALUES (:url) ON CONFLICT DO NOTHING'),
            dict(url=url)
        )
    db.commit()
