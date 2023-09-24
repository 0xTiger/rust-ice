import re
import time
import random
from itertools import count

from sqlalchemy import text
from bs4 import BeautifulSoup
from selenium.webdriver import Chrome, ActionChains
from selenium.webdriver.common.by import By
from selenium.common.exceptions import (
    NoSuchWindowException,
    MoveTargetOutOfBoundsException,
    StaleElementReferenceException
)

from db import db_ctx
from scraper import bcolors


url = 'https://groceries.asda.com/'
browser = Chrome()
browser.get(url)


def scroll_to_element(driver, element_locator):
    print(f'scrolling to {element_locator.text}')
    actions = ActionChains(driver)
    try:
        actions.move_to_element(element_locator).perform()
    except MoveTargetOutOfBoundsException as e:
        print(e)
        driver.execute_script("arguments[0].scrollIntoView(true);", element_locator)


def accept_cookies():
    print('accept_cookies')
    clickable = browser.find_element(By.ID, "onetrust-accept-btn-handler")
    ActionChains(browser)\
        .click(clickable)\
        .perform()


def open_menu():
    print('open_menu')
    clickable = browser.find_element(By.CLASS_NAME, "menu-button")
    ActionChains(browser)\
        .click(clickable)\
        .perform()


def goto_groceries():
    print('goto_groceries')
    clickable = browser.find_element(By.XPATH, "//button[text()='Groceries']")
    ActionChains(browser)\
        .click(clickable)\
        .perform()

def back_to_groceries():
    print('back_to_groceries')
    # clickable = browser.find_element(By.XPATH, "//button[text()='Shop Groceries']")
    # scroll_to_element(browser, clickable)
    # # print(clickable.text)
    # ActionChains(browser)\
    #     .click(clickable)\
    #     .perform()
    clickable = [x for x in browser.find_elements(By.CLASS_NAME, "asda-slide-nav__item-container") if x.text == 'Shop Groceries'][0]
    ActionChains(browser).click(clickable).perform()
    

def nav_to_random_menu_item():
    print('nav_to_random_menu_item', flush=True, end='')
    menu_items = browser.find_elements(By.CLASS_NAME, "slide-navigation-menu__item")
    print([m.text for m in menu_items])
    if not menu_items:
        print(' - None found')
        return 'MENU_NOT_FOUND'
    valid_menu_items = [item for item in menu_items if item.text not in {'', 'Asda Rewards'} | visited_cats]
    if not valid_menu_items:
        print(' - None valid')
        return 'MENU_EXHAUSTED'
    clickable = random.choice(valid_menu_items)
    clickable_text = clickable.text
    print(f' - {clickable_text}')
    scroll_to_element(browser, clickable)
    ActionChains(browser)\
        .click(clickable)\
        .perform()
    visited_cats.add(clickable_text)
    return 'SUCCESS'


def save_product_urls():
    with db_ctx() as db:
        for url in found_products:
            db.execute(
                text('INSERT INTO productscrapestatus (url) VALUES (:url) ON CONFLICT DO NOTHING'),
                dict(url=url)
            )
        db.commit()


time.sleep(0.5)
accept_cookies()
time.sleep(0.5)
open_menu()
time.sleep(0.5)
goto_groceries()
time.sleep(0.5)
with db_ctx() as db:
    existing_products = set(db.execute(text('SELECT url FROM product')).scalars())

visited_cats = set()
found_products = set()
found_cats = set()
for i in count():
    try:
        soup = BeautifulSoup(browser.page_source, features="html.parser")
        found_products |= {'https://groceries.asda.com/product/' + x.group(1) 
            for x in re.finditer(r'/product/([\-/a-zA-Z0-9]+)', str(soup))
        }
        found_cats |= {'https://groceries.asda.com/cat/' + x.group(1) 
            for x in re.finditer(r'/cat/([menu_items\-/a-zA-Z0-9]+)', str(soup))
        }
        infostr = f'{len(found_cats)} {len(found_products)} {bcolors.GREEN}+({len(found_products - existing_products)}){bcolors.ENDC}'
        print(infostr, flush=True, end='\r')
        time.sleep(0.1)

        if i % 5 == 0:
            time.sleep(0.5)
            try:
                status = nav_to_random_menu_item()
                if status == 'MENU_NOT_FOUND':
                    time.sleep(0.5)
                    open_menu()
                    goto_groceries()
                if status == 'MENU_EXHAUSTED':
                    time.sleep(0.5)
                    back_to_groceries()
            except (MoveTargetOutOfBoundsException, StaleElementReferenceException):
                time.sleep(0.5)
        
        if i % 20 == 0:
            save_product_urls()
    except (KeyboardInterrupt, NoSuchWindowException):
        break
browser.quit()

save_product_urls()