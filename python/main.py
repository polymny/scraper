#!/usr/bin/env python

import os.path
import sys
import json
from json.decoder import JSONDecodeError

import platformdirs

# Disable YOLO output
os.environ['YOLO_VERBOSE'] = 'False'
from ultralytics import YOLO

WEIGHTS_DIRECTORY = os.path.join(platformdirs.user_cache_dir('gbif-scraper', ensure_exists=True))
WEIGHTS_PATH = os.path.join(WEIGHTS_DIRECTORY, 'weights.pt')
WEIGHTS_URL = "https://github.com/edgaremy/quick-detector/raw/refs/heads/main/models/arthropod_dectector_wave18_best.pt"


def eprint(*args, **kwargs):
    print(*args, file=sys.stderr, **kwargs, flush=True)


class Request:
    @staticmethod
    def parse(json_data):
        ty = json_data.get('type', None)

        if ty == 'add_file':
            return AddFile(json_data)
        elif ty == 'run':
            return Run(json_data)
        elif ty == 'end':
            return End(json_data)
        else:
            eprint(f"unknown message type: {ty}")
            return None


class AddFile(Request):
    def __init__(self, json_data):
        self.ty = 'add_file'
        self.id = json_data['id']
        self.path = json_data['path']

    def __repr__(self):
        return f"AddFile(id={self.id}, path={self.path})"


class Run(Request):
    def __init__(self, json_data):
        self.ty = 'run'

    def __repr__(self):
        return f"Run()"


class End(Request):
    def __init__(self, json_data):
        self.ty = 'end'

    def __repr__(self):
        return f"End()"


class Response:
    def to_json(self):
        return json.dumps(self.__dict__)


class FileCropSuccess(Response):
    def __init__(self, id, path, box, confidence):
        self.type = "file_crop_success"
        self.id = id
        self.path = path
        self.x = box[0]
        self.y = box[1]
        self.width = box[2]
        self.height = box[3]
        self.confidence = confidence


class FileCropFailure(Response):
    def __init__(self, id, path):
        self.type = "file_crop_failure"
        self.id = id
        self.path = path


def main():
    # Download YOLO weights if not present
    if not os.path.exists(WEIGHTS_PATH):
        eprint('weights not found, downloading')
        try:
            os.mkdir(WEIGHTS_DIRECTORY)
        except FileExistsError:
            pass

        try:
            urllib.request.urlretrieve(WEIGHTS_URL, WEIGHTS_PATH)
        except urllib.error.HTTPError as e:
            eprint(f"failed to download weights: {e.code} {e.reason} {e.url}")
            sys.exit(1)

        eprint('weights downloaded')

    model = YOLO(WEIGHTS_PATH)


    batch = []
    while True:
        try:
            request_str = input()
        except EOFError:
            return

        try:
            request_object = json.loads(request_str)
        except JSONDecodeError:
            eprint(f"failed to parse json: {request_str}")
            continue

        request = Request.parse(request_object)

        if request.ty == 'add_file':
            batch.append(request)

        elif request.ty == 'run' or request.ty == 'end':

            if len(batch) > 0:
                responses = []

                images = list(map(lambda x: x.path, batch))
                predicts = model.predict(images, save_crop=False, show=False, save=False, save_txt=False, max_det=1)

                for index, (request, predict) in enumerate(zip(batch, predicts)):

                    if len(predict.boxes) > 0:
                        box = predict.boxes[0].xywh[0].tolist()
                        confidence = predict.boxes[0].conf.tolist()[0]
                        response = FileCropSuccess(request.id, request.path, box, confidence)
                    else:
                        response  = FileCropFailure(request.id, request.path)
                    responses.append(response)

                print('[' + ", ".join(list(map(lambda x: x.to_json(), responses))) + ']', flush=True)

                batch = []

            if request.ty == 'end':
                return


if __name__ == '__main__':
    main()
