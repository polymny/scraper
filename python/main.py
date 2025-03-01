#!/usr/bin/env python

import os.path
import sys
import tempfile
import glob
import json
from pathlib import Path
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


class Batch:
    def __init__(self, id):
        self.id = id
        self.files = []

    def to_json(self):
        return f'{{"id": {self.id}, "files": [' + ','.join([x.to_json() for x in self.files]) + ']}'


class FileCropSuccess(Response):
    def __init__(self, id, path, cropped_path, box, confidence):
        self.type = "file_crop_success"
        self.id = id
        self.path = path
        self.cropped_path = cropped_path
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


def main(tmp_root):
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

    os.makedirs(tmp_root, exist_ok=True)

    model = YOLO(WEIGHTS_PATH)

    batch_counter = 0
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
                batch_response = Batch(batch_counter)

                images = list(map(lambda x: x.path, batch))
                tmp_dir = os.path.join(tmp_root, str(batch_counter))
                os.makedirs(tmp_dir, exist_ok=True)
                predicts = model.predict(images, save_crop=True, show=False, save=False, save_txt=False, max_det=1, project=tmp_dir)

                for index, (request, predict) in enumerate(zip(batch, predicts)):

                    if len(predict.boxes) > 0:
                        box = predict.boxes[0].xywh[0].tolist()
                        confidence = predict.boxes[0].conf.tolist()[0]
                        filename = Path(request.path).stem

                        files = glob.glob(f'{tmp_dir}/**/{filename}.*', recursive=True)

                        # Find cropped media in directory
                        if len(files) == 0:
                            eprint(f"python error: crop succeeded but file was not created for {filename}")
                            continue

                        response = FileCropSuccess(request.id, request.path, files[0], box, confidence)
                    else:
                        response  = FileCropFailure(request.id, request.path)

                    batch_response.files.append(response)

                print(batch_response.to_json(), flush=True)

                batch = []
                batch_counter += 1

            if request.ty == 'end':
                return


if __name__ == '__main__':
    main(sys.argv[1])
